use super::eval::{compare_values, values_equal};
use super::place::{
    expr_to_lvalue, format_place_label, read_at_path, write_resolved_place, ResolvedPlace,
    ResolvedStep, ScopeFrame, SlotBinding, VarSlot,
};
use super::sandbox::{activate_arquivo, write_line_to_arquivo, FileMode};
use super::snapshot::snapshot_debug_from;
use super::{parse_read_value, vm_runtime_err, ConsoleColorTarget, Interpreter, VmError, VmResult};
use crate::ast::{
    AccessStep, AleatorioConfig, CaseArm, CaseMatcher, Expr, LValue, PrimitiveType, Stmt,
    TimerConfig, WriteArg,
};
use crate::builtins::{call_builtin, is_builtin_name};
use crate::error::{Result, Span, AlgError};
use crate::lexer::normalize_keyword;
use crate::value::{ensure_string_len, ResolvedType, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub(crate) const MAX_EXEC_BLOCK_DEPTH: usize = 768;

#[derive(Debug)]
pub(crate) enum ExecSignal {
    Next,
    Break(Box<Span>),
    Return(Box<(Option<Value>, Span)>),
}

trait ForControl: Copy {
    fn to_value(self) -> Value;
    fn should_run(self) -> bool;
    fn advance(&mut self, binding: &SlotBinding, var: &str, span: Span) -> VmResult<bool>;
}

#[derive(Clone, Copy)]
struct IntForControl {
    current: i64,
    end: i64,
    step: i64,
}

impl ForControl for IntForControl {
    fn to_value(self) -> Value {
        Value::Inteiro(self.current)
    }

    fn should_run(self) -> bool {
        if self.step > 0 {
            self.current <= self.end
        } else {
            self.current >= self.end
        }
    }

    fn advance(&mut self, binding: &SlotBinding, _var: &str, _span: Span) -> VmResult<bool> {
        let after_body = match binding.read_value()? {
            Value::Inteiro(n) => n,
            _ => self.current,
        };
        match after_body.checked_add(self.step) {
            Some(next) => {
                self.current = next;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

#[derive(Clone, Copy)]
struct RealForControl {
    current: f64,
    end: f64,
    step: f64,
}

impl ForControl for RealForControl {
    fn to_value(self) -> Value {
        Value::Real(self.current)
    }

    fn should_run(self) -> bool {
        if self.step > 0.0 {
            self.current <= self.end
        } else {
            self.current >= self.end
        }
    }

    fn advance(&mut self, binding: &SlotBinding, var: &str, span: Span) -> VmResult<bool> {
        let after_body = binding.read_value()?.as_number(span)?;
        if !after_body.is_finite() {
            return Err(vm_runtime_err(
                Some(span),
                format!("Variável de controle '{var}' tornou-se não finita no laço 'para'."),
            ));
        }
        let next = after_body + self.step;
        if !next.is_finite() {
            Ok(false)
        } else {
            self.current = next;
            Ok(true)
        }
    }
}

impl<'a> Interpreter<'a> {
    #[inline(never)]
    pub(crate) fn before_stmt_hook(&mut self, line: usize) -> Result<()> {
        let depth = self.frames.len();
        {
            let prog_name = &self.program.title;
            let globals = &self.globals;
            let frames = &self.frames;
            let mut get_snap = || snapshot_debug_from(prog_name, globals, frames);
            self.debug.before_statement(line, depth, &mut get_snap)?;
        }
        if self.timer_ms > 0 {
            self.before_stmt_timer_slow(line, depth)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn before_stmt_timer_slow(&mut self, line: usize, depth: usize) -> Result<()> {
        let prog_name = &self.program.title;
        let globals = &self.globals;
        let frames = &self.frames;
        let mut get_snap = || snapshot_debug_from(prog_name, globals, frames);
        self.debug.on_timer_trace(line, depth, &mut get_snap)?;
        self.sleep_interruptible(self.timer_ms)
    }

    #[inline(never)]
    fn trigger_pause_directive(&mut self, line: usize) -> Result<()> {
        let depth = self.frames.len();
        let prog_name = &self.program.title;
        let globals = &self.globals;
        let frames = &self.frames;
        let mut get_snap = || snapshot_debug_from(prog_name, globals, frames);
        self.debug.pause_directive(line, depth, &mut get_snap)
    }

    fn sleep_interruptible(&self, total_ms: u64) -> Result<()> {
        let mut remaining = total_ms;
        while remaining > 0 {
            self.debug.check_cancel()?;
            let chunk = remaining.min(20);
            std::thread::sleep(Duration::from_millis(chunk));
            remaining -= chunk;
        }
        self.debug.check_cancel()
    }

    #[inline(never)]
    pub(crate) fn exec_block(&mut self, stmts: &[Stmt]) -> VmResult<ExecSignal> {
        if self.block_depth >= MAX_EXEC_BLOCK_DEPTH {
            return Err(block_depth_exceeded_err(stmts.first().map(Stmt::span)));
        }
        self.block_depth += 1;
        let res = self.exec_block_loop(stmts);
        self.block_depth -= 1;
        res
    }

    #[inline(always)]
    fn exec_block_loop(&mut self, stmts: &[Stmt]) -> VmResult<ExecSignal> {
        for stmt in stmts {
            self.before_stmt_hook(stmt.line())?;
            match self.exec_stmt(stmt) {
                Ok(ExecSignal::Next) => {}
                other => return other,
            }
        }
        Ok(ExecSignal::Next)
    }

    #[inline(never)]
    fn exec_stmt(&mut self, stmt: &Stmt) -> VmResult<ExecSignal> {
        match stmt {
            Stmt::If {
                cond,
                then_body,
                else_body,
                ..
            } => self.exec_if(cond, then_body, else_body),
            Stmt::Return { value, span } => self.exec_return(value.as_ref(), *span),
            Stmt::ProcCall { name, args, span } => self.exec_proc_call(name, args, *span),
            other => self.exec_stmt_other(other),
        }
    }

    #[inline(never)]
    fn exec_return(&mut self, value: Option<&Expr>, span: Span) -> VmResult<ExecSignal> {
        let v = match value {
            Some(e) => Some(self.eval_expr_depth(e, 0)?),
            None => None,
        };
        Ok(ExecSignal::Return(Box::new((v, span))))
    }

    #[inline(never)]
    fn exec_proc_call(&mut self, name: &str, args: &[Expr], span: Span) -> VmResult<ExecSignal> {
        self.invoke_routine(name, args, span, false)?;
        Ok(ExecSignal::Next)
    }

    #[inline(never)]
    fn exec_stmt_other(&mut self, stmt: &Stmt) -> VmResult<ExecSignal> {
        match stmt {
            Stmt::Assign {
                target,
                value,
                span,
            } => self.exec_assign(target, value, *span),
            Stmt::Break { span } => Ok(ExecSignal::Break(Box::new(*span))),
            Stmt::Write {
                args,
                newline,
                span,
            } => self.exec_write(args, *newline, *span),
            Stmt::Read { targets, .. } => self.exec_read_stmt(targets),
            Stmt::ClearScreen { .. } => {
                self.io.clear();
                Ok(ExecSignal::Next)
            }
            Stmt::Switch {
                expr,
                arms,
                otherwise,
                ..
            } => self.exec_switch(expr, arms, otherwise),
            Stmt::For {
                var,
                start,
                end,
                step,
                body,
                span,
            } => self.exec_for(var, start, end, step.as_ref(), body, *span),
            Stmt::While { cond, body, span } => self.exec_while(cond, body, *span),
            Stmt::Repeat {
                body, until_cond, ..
            } => self.exec_repeat(body, until_cond),
            Stmt::Arquivo { path, span } => self.exec_arquivo_stmt(path, *span),
            Stmt::Aleatorio { config, .. } => self.exec_aleatorio(config),
            Stmt::Timer { config, .. } => self.exec_timer(config),
            Stmt::Cronometro { enabled, .. } => self.exec_cronometro(*enabled),
            Stmt::Eco { enabled, .. } => {
                self.echo_enabled = *enabled;
                Ok(ExecSignal::Next)
            }
            Stmt::Pausa { span } => {
                self.trigger_pause_directive(span.line)?;
                Ok(ExecSignal::Next)
            }
            Stmt::Debug { cond, span } => self.exec_debug(cond.as_ref(), *span),
            Stmt::MudaCor {
                color,
                target,
                span,
            } => self.exec_mudacor(color, target, *span),
            Stmt::Dos { .. } => self.exec_dos(),
            Stmt::If { .. } | Stmt::Return { .. } | Stmt::ProcCall { .. } => Ok(ExecSignal::Next),
        }
    }

    #[inline(never)]
    fn exec_assign(&mut self, target: &LValue, value: &Expr, span: Span) -> VmResult<ExecSignal> {
        let val = self.eval_expr(value)?;
        self.assign_lvalue(target, val, span)?;
        Ok(ExecSignal::Next)
    }

    #[inline(never)]
    fn exec_arquivo_stmt(&mut self, path: &str, span: Span) -> VmResult<ExecSignal> {
        activate_arquivo(
            self.options.sandbox_dir.as_deref(),
            &mut self.file_mode,
            path,
            span,
        )?;
        Ok(ExecSignal::Next)
    }

    #[inline(never)]
    fn exec_dos(&mut self) -> VmResult<ExecSignal> {
        self.io.write_error(
            "Por segurança, o comando 'dos' (execução externa no sistema operacional) é desativado no AlgoritmosLab.",
        );
        Ok(ExecSignal::Next)
    }

    #[inline(never)]
    fn exec_read_stmt(&mut self, targets: &[LValue]) -> VmResult<ExecSignal> {
        for target in targets {
            self.read_into_lvalue(target)?;
        }
        Ok(ExecSignal::Next)
    }

    #[inline(never)]
    fn eval_cond_bool(&mut self, cond: &Expr) -> VmResult<bool> {
        Ok(self.eval_expr(cond)?.as_bool(cond.span())?)
    }

    #[inline(never)]
    fn exec_if(&mut self, cond: &Expr, then_body: &[Stmt], else_body: &[Stmt]) -> VmResult<ExecSignal> {
        let c = self.eval_cond_bool(cond)?;
        if c {
            self.exec_block(then_body)
        } else if !else_body.is_empty() {
            self.exec_block(else_body)
        } else {
            Ok(ExecSignal::Next)
        }
    }

    #[inline(never)]
    fn exec_while(&mut self, cond: &Expr, body: &[Stmt], span: Span) -> VmResult<ExecSignal> {
        let mut first_iter = true;
        loop {
            if !first_iter {
                self.before_stmt_hook(span.line)?;
            } else {
                self.debug.check_cancel()?;
            }
            first_iter = false;

            let c = self.eval_expr(cond)?.as_bool(cond.span())?;
            if !c {
                break;
            }
            match self.exec_block(body)? {
                ExecSignal::Next => {}
                ExecSignal::Break(_) => break,
                ret @ ExecSignal::Return(_) => return Ok(ret),
            }
        }
        Ok(ExecSignal::Next)
    }

    #[inline(never)]
    fn exec_repeat(&mut self, body: &[Stmt], until_cond: &Expr) -> VmResult<ExecSignal> {
        loop {
            self.debug.check_cancel()?;
            match self.exec_block(body)? {
                ExecSignal::Next => {}
                ExecSignal::Break(_) => break,
                ret @ ExecSignal::Return(_) => return Ok(ret),
            }
            self.before_stmt_hook(until_cond.span().line)?;
            let done = self.eval_expr(until_cond)?.as_bool(until_cond.span())?;
            if done {
                break;
            }
        }
        Ok(ExecSignal::Next)
    }

    #[inline(never)]
    fn exec_aleatorio(&mut self, config: &AleatorioConfig) -> VmResult<ExecSignal> {
        match config {
            AleatorioConfig::OnDefault => {
                self.random_input = Some((0, 100));
            }
            AleatorioConfig::Off => {
                self.random_input = None;
            }
            AleatorioConfig::MaxOnly(max_e) => {
                let m = self.eval_expr(max_e)?.as_int(max_e.span())?;
                let lo = 0.min(m);
                let hi = 0.max(m);
                self.random_input = Some((lo, hi));
            }
            AleatorioConfig::Range(a_e, b_e) => {
                let a = self.eval_expr(a_e)?.as_int(a_e.span())?;
                let b = self.eval_expr(b_e)?.as_int(b_e.span())?;
                self.random_input = Some((a.min(b), a.max(b)));
            }
        }
        Ok(ExecSignal::Next)
    }

    #[inline(never)]
    fn exec_timer(&mut self, config: &TimerConfig) -> VmResult<ExecSignal> {
        match config {
            TimerConfig::On => {
                self.timer_ms = 500;
            }
            TimerConfig::Off => {
                self.timer_ms = 0;
            }
            TimerConfig::Millis(ms_e) => {
                let ms = self.eval_expr(ms_e)?.as_int(ms_e.span())?;
                if ms < 0 {
                    return Err(vm_runtime_err(
                        Some(ms_e.span()),
                        "O atraso do comando 'timer' não pode ser negativo.",
                    ));
                }
                self.timer_ms = (ms as u64).min(10_000);
            }
        }
        Ok(ExecSignal::Next)
    }

    #[inline(never)]
    fn exec_cronometro(&mut self, enabled: bool) -> VmResult<ExecSignal> {
        if enabled {
            self.stopwatch_start = Some(Instant::now());
            self.io.write("Cronômetro iniciado.\n");
        } else {
            let elapsed = self
                .stopwatch_start
                .take()
                .map(|s| s.elapsed())
                .unwrap_or_default();
            let secs = elapsed.as_secs();
            let ms = elapsed.subsec_millis();
            self.io.write(&format!(
                "Cronômetro terminado. Tempo decorrido: {secs} segundo(s) e {ms} ms\n"
            ));
        }
        Ok(ExecSignal::Next)
    }

    #[inline(never)]
    fn exec_debug(&mut self, cond: Option<&Expr>, span: Span) -> VmResult<ExecSignal> {
        let should_pause = match cond {
            Some(e) => self.eval_expr(e)?.as_bool(e.span())?,
            None => true,
        };
        if should_pause {
            self.trigger_pause_directive(span.line)?;
        }
        Ok(ExecSignal::Next)
    }

    #[inline(never)]
    fn exec_mudacor(&mut self, color: &Expr, target: &Expr, span: Span) -> VmResult<ExecSignal> {
        let color_raw = self.eval_expr(color)?.as_str(color.span())?.to_string();
        let target_raw = self.eval_expr(target)?.as_str(target.span())?.to_string();
        let color_target = parse_color_target(&target_raw, target.span())?;
        let color_hex = resolve_console_color(&color_raw, span)?;
        self.io
            .set_console_color(color_target, &color_hex, color_raw.trim());
        Ok(ExecSignal::Next)
    }

    #[inline(never)]
    fn exec_write(&mut self, args: &[WriteArg], newline: bool, span: Span) -> VmResult<ExecSignal> {
        let mut buf = String::new();
        for arg in args {
            let val = self.eval_expr(&arg.expr)?;
            let w = match &arg.width {
                Some(we) => Some(self.eval_expr(we)?.as_int(we.span())?),
                None => None,
            };
            let d = match &arg.decimals {
                Some(de) => Some(self.eval_expr(de)?.as_int(de.span())?),
                None => None,
            };
            let formatted = val.format_write(w, d, span)?;
            ensure_string_len(buf.len().saturating_add(formatted.len()), Some(span))?;
            buf.push_str(&formatted);
        }
        if newline {
            ensure_string_len(buf.len().saturating_add(1), Some(span))?;
            buf.push('\n');
        }
        self.io.write(&buf);
        Ok(ExecSignal::Next)
    }

    #[inline(never)]
    fn exec_switch(
        &mut self,
        expr: &Expr,
        arms: &[CaseArm],
        otherwise: &[Stmt],
    ) -> VmResult<ExecSignal> {
        let switch_val = self.eval_expr(expr)?;
        for arm in arms {
            let mut matched = false;
            for matcher in &arm.matchers {
                match matcher {
                    CaseMatcher::Value(e) => {
                        let v = self.eval_expr(e)?;
                        if values_equal(&switch_val, &v, e.span())? {
                            matched = true;
                            break;
                        }
                    }
                    CaseMatcher::Range(start_e, end_e) => {
                        let start_v = self.eval_expr(start_e)?;
                        let end_v = self.eval_expr(end_e)?;
                        let ge_start = compare_values(&switch_val, &start_v, start_e.span())?
                            != std::cmp::Ordering::Less;
                        let le_end = compare_values(&switch_val, &end_v, end_e.span())?
                            != std::cmp::Ordering::Greater;
                        if ge_start && le_end {
                            matched = true;
                            break;
                        }
                    }
                }
            }
            if matched {
                return self.exec_block(&arm.body);
            }
        }
        if !otherwise.is_empty() {
            self.exec_block(otherwise)
        } else {
            Ok(ExecSignal::Next)
        }
    }

    #[inline(never)]
    fn exec_for(
        &mut self,
        var: &str,
        start: &Expr,
        end: &Expr,
        step: Option<&Expr>,
        body: &[Stmt],
        span: Span,
    ) -> VmResult<ExecSignal> {
        let start_raw = self.eval_expr(start)?;
        let end_raw = self.eval_expr(end)?;
        let step_raw = match step {
            Some(se) => Some((self.eval_expr(se)?, se.span())),
            None => None,
        };

        let binding = self.lookup_binding(var, span)?;
        if binding.is_const() {
            return Err(vm_runtime_err(
                Some(span),
                format!(
                    "Constante '{var}' não pode ser usada como variável de controle do laço 'para'."
                ),
            ));
        }

        let control_prim = match binding.resolved_type() {
            ResolvedType::Primitive(PrimitiveType::Inteiro) => PrimitiveType::Inteiro,
            ResolvedType::Primitive(PrimitiveType::Real) => PrimitiveType::Real,
            _ => {
                return Err(vm_runtime_err(
                    Some(span),
                    format!("Variável de controle '{var}' no laço 'para' deve ser numérica."),
                ));
            }
        };

        if control_prim == PrimitiveType::Real {
            let start_val = start_raw.as_number(start.span())?;
            let end_val = end_raw.as_number(end.span())?;
            let step_val = match step_raw {
                Some((v, sp)) => v.as_number(sp)?,
                None => 1.0,
            };
            if !start_val.is_finite() || !end_val.is_finite() || !step_val.is_finite() {
                return Err(vm_runtime_err(
                    Some(span),
                    "Limites e passo do laço 'para' devem ser valores finitos.",
                ));
            }
            if step_val == 0.0 {
                return Err(vm_runtime_err(
                    Some(span),
                    "O passo do laço 'para' não pode ser zero.",
                ));
            }

            return self.exec_for_loop(
                &binding,
                var,
                RealForControl {
                    current: start_val,
                    end: end_val,
                    step: step_val,
                },
                body,
                span,
            );
        }

        // Controle inteiro (aceita Real exato como `10 / 2 = 5.0`)
        let start_val = value_to_exact_int(&start_raw, start.span(), "início")?;
        let end_val = value_to_exact_int(&end_raw, end.span(), "fim")?;
        let step_val = match step_raw {
            Some((v, sp)) => value_to_exact_int(&v, sp, "passo")?,
            None => 1,
        };
        if step_val == 0 {
            return Err(vm_runtime_err(
                Some(span),
                "O passo do laço 'para' não pode ser zero.",
            ));
        }

        self.exec_for_loop(
            &binding,
            var,
            IntForControl {
                current: start_val,
                end: end_val,
                step: step_val,
            },
            body,
            span,
        )
    }

    fn exec_for_loop<C: ForControl>(
        &mut self,
        binding: &SlotBinding,
        var: &str,
        mut ctrl: C,
        body: &[Stmt],
        span: Span,
    ) -> VmResult<ExecSignal> {
        binding.write_value(ctrl.to_value(), span)?;
        let mut first_iter = true;
        loop {
            if !first_iter {
                self.before_stmt_hook(span.line)?;
            } else {
                self.debug.check_cancel()?;
            }
            first_iter = false;

            if !ctrl.should_run() {
                break;
            }

            binding.write_value(ctrl.to_value(), span)?;

            match self.exec_block(body)? {
                ExecSignal::Next => {}
                ExecSignal::Break(_) => break,
                ret @ ExecSignal::Return(_) => return Ok(ret),
            }

            if !ctrl.advance(binding, var, span)? {
                break;
            }
        }
        Ok(ExecSignal::Next)
    }

    pub(crate) fn resolve_place(&mut self, target: &LValue) -> VmResult<ResolvedPlace> {
        self.resolve_place_depth(target, 0)
    }

    pub(crate) fn resolve_place_depth(
        &mut self,
        target: &LValue,
        depth: usize,
    ) -> VmResult<ResolvedPlace> {
        let mut local_steps = Vec::with_capacity(target.steps.len());
        for step in &target.steps {
            match step {
                AccessStep::Field { name, span } => {
                    local_steps.push(ResolvedStep::Field(name.clone(), *span));
                }
                AccessStep::Index { indices, span } => {
                    let idx_vals = self.eval_indices_depth(indices, depth)?;
                    local_steps.push(ResolvedStep::Index(idx_vals, *span));
                }
            }
        }
        let display_label = format_place_label(&target.base, &local_steps);
        let binding = self.lookup_binding(&target.base, target.base_span)?;
        match binding {
            SlotBinding::Slot { slot, .. } => Ok(ResolvedPlace {
                slot,
                base_name: target.base.clone(),
                display_label,
                steps: local_steps,
                span: target.span,
            }),
            SlotBinding::SubPlace {
                slot, mut steps, ..
            } => {
                steps.extend(local_steps);
                Ok(ResolvedPlace {
                    slot,
                    base_name: target.base.clone(),
                    display_label,
                    steps,
                    span: target.span,
                })
            }
        }
    }

    fn assign_lvalue(&mut self, target: &LValue, value: Value, span: Span) -> Result<()> {
        let place = self.resolve_place(target).map_err(VmError::into_error)?;
        write_resolved_place(&place, value, span)
    }

    fn read_into_lvalue(&mut self, target: &LValue) -> Result<()> {
        let place = self.resolve_place(target).map_err(VmError::into_error)?;
        {
            let s = place.slot.borrow();
            if s.is_const {
                return Err(AlgError::runtime(
                    Some(place.span),
                    format!(
                        "Não é possível ler para a constante '{}' com 'leia'.",
                        place.base_name
                    ),
                ));
            }
        }
        let target_label = &place.display_label;
        let expected_type = {
            let s = place.slot.borrow();
            let current_val = read_at_path(&s.value, &place.base_name, &place.steps)?;
            match current_val.resolved_type() {
                ResolvedType::Primitive(p) => p,
                ResolvedType::Array { .. } => {
                    return Err(AlgError::runtime(
                        Some(place.span),
                        format!(
                            "Use índices para ler elemento do vetor '{target_label}' (ex.: leia({target_label}[1])).",
                        ),
                    ));
                }
                ResolvedType::Record(_) => {
                    return Err(AlgError::runtime(
                        Some(place.span),
                        format!(
                            "Especifique um campo do registro '{target_label}' em 'leia' (ex.: leia({target_label}.campo)).",
                        ),
                    ));
                }
            }
        };

        // 1) Modo `aleatorio` ativo -> gera valor diretamente
        if let Some((min, max)) = self.random_input {
            self.debug.check_cancel()?;
            let generated = self.generate_random_value(min, max, expected_type);
            let display_str = generated.to_display_string();
            if self.echo_enabled {
                self.io.echo_input(&display_str);
            }
            write_line_to_arquivo(&mut self.file_mode, &display_str, place.span)?;
            return write_resolved_place(&place, generated, place.span);
        }

        // 2) Modo `arquivo` existente (leitura sequencial linha a linha)
        if let FileMode::Reading {
            display_name,
            lines,
            line_no,
        } = &mut self.file_mode
        {
            if let Some(raw_line) = lines.pop_front() {
                *line_no += 1;
                let cur_line_no = *line_no;
                let file_label = display_name.clone();
                self.debug.check_cancel()?;
                let parsed = parse_read_value(&raw_line, expected_type).map_err(|msg| {
                    AlgError::runtime(
                        Some(place.span),
                        format!("Arquivo '{file_label}' (linha {cur_line_no}): {msg}"),
                    )
                })?;
                if self.echo_enabled {
                    self.io.echo_input(&raw_line);
                }
                return write_resolved_place(&place, parsed, place.span);
            } else {
                let ended_name = display_name.clone();
                self.file_mode = FileMode::None;
                self.io.write_error(&format!(
                    "Fim do arquivo '{ended_name}' atingido; continuando leitura pelo teclado."
                ));
            }
        }

        // 3) Leitura interativa pelo teclado (com gravação preguiçosa opcional em arquivo novo)
        loop {
            self.debug.check_cancel()?;
            let raw = self
                .io
                .read_input(target_label, expected_type.label(), place.span.line)?;

            match parse_read_value(&raw, expected_type) {
                Ok(parsed) => {
                    if self.echo_enabled {
                        self.io.echo_input(&raw);
                    }
                    write_line_to_arquivo(&mut self.file_mode, &raw, place.span)?;
                    return write_resolved_place(&place, parsed, place.span);
                }
                Err(msg) => {
                    if self.options.strict_read {
                        return Err(AlgError::runtime(Some(place.span), msg));
                    }
                    self.io.write_error(&format!(
                        "{msg} Digite novamente o valor para '{target_label}'."
                    ));
                }
            }
        }
    }

    fn generate_random_value(&mut self, min: i64, max: i64, expected: PrimitiveType) -> Value {
        match expected {
            PrimitiveType::Inteiro => {
                let span = (max as i128 - min as i128 + 1).max(1) as u128;
                let offset = (self.rng.next_u64() as u128) % span;
                Value::Inteiro((min as i128 + offset as i128) as i64)
            }
            PrimitiveType::Real => {
                let raw = min as f64 + self.rng.next_f64() * (max as f64 - min as f64);
                let rounded = (raw * 100.0).round() / 100.0;
                Value::Real(rounded)
            }
            PrimitiveType::Caractere => {
                let mut s = String::with_capacity(5);
                for _ in 0..5 {
                    let ch = (b'A' + (self.rng.next_u64() % 26) as u8) as char;
                    s.push(ch);
                }
                Value::Caractere(s)
            }
            PrimitiveType::Logico => Value::Logico(self.rng.next_u64().is_multiple_of(2)),
        }
    }

    pub(crate) fn invoke_routine(
        &mut self,
        name: &str,
        args: &[Expr],
        span: Span,
        expect_return: bool,
    ) -> VmResult<Option<Value>> {
        self.invoke_routine_depth(name, args, span, expect_return, 0)
    }

    #[inline(never)]
    pub(crate) fn invoke_routine_depth(
        &mut self,
        name: &str,
        args: &[Expr],
        span: Span,
        expect_return: bool,
        eval_depth: usize,
    ) -> VmResult<Option<Value>> {
        if is_builtin_name(name) {
            return self.invoke_builtin_depth(name, args, span, expect_return, eval_depth);
        }

        let routine = self.push_routine_frame(name, args, span, expect_return, eval_depth)?;
        let sig = self.exec_block(&routine.body)?;
        self.finish_routine_call(routine, sig, span)
    }

    #[inline(never)]
    fn invoke_builtin_depth(
        &mut self,
        name: &str,
        args: &[Expr],
        span: Span,
        expect_return: bool,
        eval_depth: usize,
    ) -> VmResult<Option<Value>> {
        if !expect_return {
            return Err(vm_runtime_err(
                Some(span),
                format!(
                    "A função interna '{name}' retorna um valor e deve ser usada em uma expressão."
                ),
            ));
        }
        let mut evaled = Vec::with_capacity(args.len());
        for a in args {
            evaled.push(self.eval_expr_depth(a, eval_depth.saturating_add(1))?);
        }
        let val = call_builtin(name, evaled, span, &mut self.rng)?
            .ok_or_else(|| vm_runtime_err(Some(span), format!("Função interna '{name}' indisponível.")))?;
        Ok(Some(val))
    }

    #[inline(never)]
    fn push_routine_frame(
        &mut self,
        name: &str,
        args: &[Expr],
        span: Span,
        expect_return: bool,
        eval_depth: usize,
    ) -> VmResult<&'a crate::ast::RoutineDecl> {
        let key = name.to_lowercase();
        let routine = match self.routines.get(&key).copied() {
            Some(r) => r,
            None => {
                return Err(vm_runtime_err(
                    Some(span),
                    if expect_return {
                        format!("Função '{name}' não declarada.")
                    } else {
                        format!("Procedimento '{name}' não declarado.")
                    },
                ));
            }
        };

        if expect_return && routine.return_type.is_none() {
            return Err(vm_runtime_err(
                Some(span),
                format!("'{}' é um procedimento e não retorna valor.", routine.name),
            ));
        }

        if self.frames.len() >= self.options.max_call_depth {
            return Err(vm_runtime_err(
                Some(span),
                format!(
                    "Limite máximo de recursão ({}) atingido ao chamar '{}'.",
                    self.options.max_call_depth, routine.name
                ),
            ));
        }

        if args.len() != routine.params.len() {
            return Err(vm_runtime_err(
                Some(span),
                format!(
                    "'{}' esperava {} parâmetro(s), mas recebeu {}.",
                    routine.name,
                    routine.params.len(),
                    args.len()
                ),
            ));
        }

        let mut frame = ScopeFrame::new(routine.name.clone());
        let build_res: VmResult<()> = (|| {
            for (param, arg_expr) in routine.params.iter().zip(args.iter()) {
                let pkey = param.name.to_lowercase();
                let param_ty = self.resolve_runtime_type(&param.var_type)?;
                if param.by_ref {
                    let lval = expr_to_lvalue(arg_expr).ok_or_else(|| {
                        vm_runtime_err(
                            Some(arg_expr.span()),
                            format!(
                                "Parâmetro 'var {}' exige uma variável, campo de registro ou posição de vetor.",
                                param.name
                            ),
                        )
                    })?;
                    let place = self.resolve_place_depth(&lval, eval_depth.saturating_add(1))?;
                    if place.slot.borrow().is_const {
                        return Err(vm_runtime_err(
                            Some(arg_expr.span()),
                            format!(
                                "Constante '{}' não pode ser passada por referência ('var {}').",
                                lval.base, param.name
                            ),
                        ));
                    }
                    let actual_ty = if place.steps.is_empty() {
                        place.slot.borrow().resolved_type.clone()
                    } else {
                        let borrowed = place.slot.borrow();
                        read_at_path(&borrowed.value, &place.base_name, &place.steps)?
                            .resolved_type()
                    };
                    if !actual_ty.matches(&param_ty) {
                        return Err(vm_runtime_err(
                            Some(arg_expr.span()),
                            format!(
                                "Parâmetro por referência '{}' ('{}') incompatível com '{}' ('{}').",
                                param.name,
                                param_ty.type_label(),
                                lval.base,
                                actual_ty.type_label()
                            ),
                        ));
                    }
                    let binding = if place.steps.is_empty() {
                        SlotBinding::Slot {
                            display_name: param.name.clone(),
                            slot: place.slot,
                        }
                    } else {
                        SlotBinding::SubPlace {
                            display_name: param.name.clone(),
                            declared_type: param_ty,
                            slot: place.slot,
                            steps: place.steps,
                        }
                    };
                    frame.insert(pkey, binding);
                } else {
                    let val = self.eval_expr_depth(arg_expr, eval_depth.saturating_add(1))?;
                    let slot_val = val.coerce_to_resolved(&param_ty, Some(arg_expr.span()))?;
                    let cells = param_ty.checked_leaf_count().map_err(|_| {
                        vm_runtime_err(
                            Some(arg_expr.span()),
                            "Parâmetro excede o limite de memória da VM.",
                        )
                    })?;
                    self.reserve_cells(cells, arg_expr.span())?;
                    frame.allocated_cells += cells;
                    let slot = Rc::new(RefCell::new(VarSlot {
                        root_name: param.name.clone(),
                        resolved_type: param_ty,
                        is_const: false,
                        value: slot_val,
                    }));
                    frame.insert(
                        pkey,
                        SlotBinding::Slot {
                            display_name: param.name.clone(),
                            slot,
                        },
                    );
                }
            }

            for local in &routine.locals {
                let lkey = local.name.to_lowercase();
                let rtype = self.resolve_runtime_type(&local.var_type)?;
                let cells = rtype.checked_leaf_count().map_err(|_| {
                    vm_runtime_err(
                        Some(local.span),
                        format!("Variável local '{}' excede o limite de memória da VM.", local.name),
                    )
                })?;
                self.reserve_cells(cells, local.span)?;
                frame.allocated_cells += cells;
                let val = Value::allocate_resolved(&local.name, &rtype, local.span)?;
                let slot = Rc::new(RefCell::new(VarSlot {
                    root_name: local.name.clone(),
                    resolved_type: rtype,
                    is_const: false,
                    value: val,
                }));
                frame.insert(
                    lkey,
                    SlotBinding::Slot {
                        display_name: local.name.clone(),
                        slot,
                    },
                );
            }
            Ok(())
        })();

        if let Err(err) = build_res {
            self.release_cells(frame.allocated_cells);
            return Err(err);
        }

        self.frames.push(frame);
        Ok(routine)
    }

    #[inline(never)]
    fn finish_routine_call(
        &mut self,
        routine: &crate::ast::RoutineDecl,
        sig: ExecSignal,
        span: Span,
    ) -> VmResult<Option<Value>> {
        let ret_val = match sig {
            ExecSignal::Break(bspan) => {
                return Err(vm_runtime_err(
                    Some(*bspan),
                    "'interrompa' usado fora de um laço.",
                ));
            }
            ExecSignal::Return(ret) => {
                let (ret_opt, rspan) = *ret;
                if let Some(ret_ast) = &routine.return_type {
                    let ret_ty = self.resolve_runtime_type(ret_ast)?;
                    let val = ret_opt.ok_or_else(|| {
                        vm_runtime_err(
                            Some(rspan),
                            format!(
                                "Função '{}' deve retornar um valor com 'retorne <valor>'.",
                                routine.name
                            ),
                        )
                    })?;
                    Some(val.coerce_to_resolved(&ret_ty, Some(rspan))?)
                } else if ret_opt.is_some() {
                    return Err(vm_runtime_err(
                        Some(rspan),
                        format!(
                            "Procedimento '{}' não retorna valor; use apenas 'retorne'.",
                            routine.name
                        ),
                    ));
                } else {
                    None
                }
            }
            ExecSignal::Next => {
                if routine.return_type.is_some() {
                    return Err(vm_runtime_err(
                        Some(span),
                        format!("Função '{}' terminou sem executar 'retorne'.", routine.name),
                    ));
                } else {
                    None
                }
            }
        };

        if let Some(popped) = self.frames.pop() {
            self.release_cells(popped.allocated_cells);
        }
        Ok(ret_val)
    }
}

#[cold]
#[inline(never)]
fn block_depth_exceeded_err(span: Option<Span>) -> VmError {
    vm_runtime_err(
        span,
        format!(
            "Limite máximo de profundidade de execução aninhada ({MAX_EXEC_BLOCK_DEPTH}) excedido."
        ),
    )
}

fn value_to_exact_int(val: &Value, span: Span, label: &str) -> Result<i64> {
    match val {
        Value::Inteiro(n) => Ok(*n),
        Value::Real(f) => {
            if !f.is_finite() {
                return Err(AlgError::runtime(
                    Some(span),
                    format!("Valor não finito não pode ser usado como {label} no laço 'para'."),
                ));
            }
            if f.fract() != 0.0 {
                return Err(AlgError::runtime(
                    Some(span),
                    format!(
                        "Valor real não inteiro ({f}) não pode ser usado como {label} para variável de controle 'inteiro' no laço 'para' (use 'int(...)' ou variável 'real')."
                    ),
                ));
            }
            let min_f = i64::MIN as f64;
            let upper_exclusive = -(i64::MIN as f64);
            if *f < min_f || *f >= upper_exclusive {
                return Err(AlgError::runtime(
                    Some(span),
                    format!("Valor {f} fora dos limites de 'inteiro' para {label} no laço 'para'."),
                ));
            }
            Ok(*f as i64)
        }
        other => Err(AlgError::runtime(
            Some(span),
            format!(
                "Esperado valor numérico inteiro para {label} no laço 'para', mas recebeu '{}'.",
                other.type_label()
            ),
        )),
    }
}

fn parse_color_target(raw: &str, span: Span) -> Result<ConsoleColorTarget> {
    match normalize_keyword(raw.trim()).as_str() {
        "frente" | "texto" | "foreground" => Ok(ConsoleColorTarget::Foreground),
        "fundos" | "fundo" | "background" => Ok(ConsoleColorTarget::Background),
        _ => Err(AlgError::runtime(
            Some(span),
            format!(
                "Posição inválida '{}' em 'mudacor'. Use \"FRENTE\" ou \"FUNDOS\".",
                raw
            ),
        )),
    }
}

fn resolve_console_color(raw: &str, span: Span) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('#')
        && (trimmed.len() == 4 || trimmed.len() == 7)
        && trimmed[1..].chars().all(|c| c.is_ascii_hexdigit())
    {
        return Ok(trimmed.to_string());
    }
    let hex = match normalize_keyword(trimmed).as_str() {
        "" | "padrao" => "",
        "preto" | "black" => "#0f172a",
        "azul" | "blue" => "#1d4ed8",
        "azul escuro" | "azulescuro" => "#1e3a8a",
        "azul claro" | "azulclaro" => "#38bdf8",
        "verde" | "green" => "#15803d",
        "verde claro" | "verdeclaro" => "#4ade80",
        "ciano" | "cyan" | "agua" => "#0891b2",
        "vermelho" | "red" => "#dc2626",
        "vermelho claro" | "vermelhoclaro" => "#f87171",
        "magenta" | "rosa" => "#c026d3",
        "roxo" | "violeta" | "purple" => "#7e22ce",
        "marrom" | "brown" => "#92400e",
        "laranja" | "orange" => "#ea580c",
        "cinza" | "cinza claro" | "cinzaclaro" | "gray" => "#94a3b8",
        "cinza escuro" | "cinzaescuro" => "#475569",
        "amarelo" | "yellow" => "#facc15",
        "branco" | "white" => "#f8fafc",
        _ => {
            return Err(AlgError::runtime(
                Some(span),
                format!(
                    "Cor desconhecida '{}' em 'mudacor'. Exemplos: Preto, Branco, Azul, Verde, Amarelo, Vermelho.",
                    raw
                ),
            ))
        }
    };
    Ok(hex.to_string())
}
