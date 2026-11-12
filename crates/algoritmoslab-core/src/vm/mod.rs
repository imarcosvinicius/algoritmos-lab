mod eval;
mod exec;
mod place;
mod sandbox;
mod snapshot;

use self::eval::eval_const_expr;
use self::exec::ExecSignal;
use self::place::{
    resolve_named_type, resolve_var_type_spec, ScopeFrame, SlotBinding, VarSlot,
};
use self::sandbox::{activate_arquivo, flush_file_writer, FileMode};
use self::snapshot::{snapshot_debug_from, snapshot_vars_from};
use crate::ast::*;
use crate::builtins::{is_builtin_name, parse_finite_f64, RngState};
use crate::error::{Result, Span, AlgError};
use crate::lexer::normalize_keyword;
use crate::value::{ResolvedType, Value, VarSnapshot, MAX_VM_TOTAL_ELEMENTS};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub const DEFAULT_MAX_CALL_DEPTH: usize = 100;
pub const MAX_SNAPSHOT_ARRAY_ITEMS: usize = 128;

#[derive(Debug)]
pub(crate) struct VmError(pub(crate) Box<AlgError>);

impl From<AlgError> for VmError {
    #[cold]
    #[inline(never)]
    fn from(err: AlgError) -> Self {
        Self(Box::new(err))
    }
}

impl VmError {
    #[cold]
    #[inline(never)]
    pub(crate) fn into_error(self) -> AlgError {
        *self.0
    }
}

pub(crate) type VmResult<T> = std::result::Result<T, VmError>;

#[cold]
#[inline(never)]
pub(crate) fn vm_runtime_err(span: Option<Span>, message: impl Into<String>) -> VmError {
    VmError(Box::new(AlgError::runtime(span, message)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleColorTarget {
    Foreground,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugSnapshot {
    pub variables: Vec<VarSnapshot>,
    pub call_stack: Vec<String>,
}

pub trait IoHandler {
    fn write(&mut self, text: &str);
    fn clear(&mut self);
    fn write_error(&mut self, text: &str);
    fn read_input(&mut self, var_name: &str, expected_type: &str, line: usize) -> Result<String>;

    fn echo_input(&mut self, _text: &str) {}
    fn set_console_color(
        &mut self,
        _target: ConsoleColorTarget,
        _color_hex: &str,
        _color_name: &str,
    ) {
    }
}

pub trait DebugController {
    fn check_cancel(&self) -> Result<()>;
    fn before_statement(
        &mut self,
        line: usize,
        call_depth: usize,
        snapshot: &mut dyn FnMut() -> DebugSnapshot,
    ) -> Result<()>;

    fn pause_directive(
        &mut self,
        _line: usize,
        _call_depth: usize,
        _snapshot: &mut dyn FnMut() -> DebugSnapshot,
    ) -> Result<()> {
        self.check_cancel()
    }

    fn on_timer_trace(
        &mut self,
        _line: usize,
        _call_depth: usize,
        _snapshot: &mut dyn FnMut() -> DebugSnapshot,
    ) -> Result<()> {
        self.check_cancel()
    }
}

/// Controlador padrão sem pausas de depuração (com suporte opcional a cancelamento).
pub struct RunController {
    cancel_flag: Arc<AtomicBool>,
}

impl RunController {
    pub fn new(cancel_flag: Arc<AtomicBool>) -> Self {
        Self { cancel_flag }
    }

    pub fn noop() -> Self {
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl DebugController for RunController {
    fn check_cancel(&self) -> Result<()> {
        if self.cancel_flag.load(Ordering::Relaxed) {
            Err(AlgError::interrupted())
        } else {
            Ok(())
        }
    }

    fn before_statement(
        &mut self,
        _line: usize,
        _call_depth: usize,
        _snapshot: &mut dyn FnMut() -> DebugSnapshot,
    ) -> Result<()> {
        self.check_cancel()
    }
}

/// Implementação em memória para testes unitários, Golden e CLI.
pub struct MemoryIo {
    inputs: VecDeque<String>,
    pub output: String,
    pub errors: Vec<String>,
    pub echoed_inputs: Vec<String>,
    pub console_colors: Vec<(ConsoleColorTarget, String)>,
}

impl MemoryIo {
    pub fn new<I, S>(inputs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            inputs: inputs.into_iter().map(Into::into).collect(),
            output: String::new(),
            errors: Vec::new(),
            echoed_inputs: Vec::new(),
            console_colors: Vec::new(),
        }
    }
}

impl IoHandler for MemoryIo {
    fn write(&mut self, text: &str) {
        self.output.push_str(text);
    }

    fn clear(&mut self) {
        self.output.clear();
    }

    fn write_error(&mut self, text: &str) {
        self.errors.push(text.to_string());
    }

    fn read_input(&mut self, var_name: &str, _expected_type: &str, line: usize) -> Result<String> {
        self.inputs.pop_front().ok_or_else(|| {
            AlgError::runtime(
                Some(Span::new(line, 1, 1)),
                format!("Fim das entradas fornecidas ao tentar ler '{var_name}'."),
            )
        })
    }

    fn echo_input(&mut self, text: &str) {
        self.echoed_inputs.push(text.to_string());
    }

    fn set_console_color(
        &mut self,
        target: ConsoleColorTarget,
        _color_hex: &str,
        color_name: &str,
    ) {
        self.console_colors
            .push((target, color_name.to_string()));
    }
}

#[derive(Debug, Clone)]
pub struct ExecOptions {
    pub max_call_depth: usize,
    pub strict_read: bool,
    pub rng_seed: Option<u64>,
    pub sandbox_dir: Option<PathBuf>,
}

impl Default for ExecOptions {
    fn default() -> Self {
        Self {
            max_call_depth: DEFAULT_MAX_CALL_DEPTH,
            strict_read: false,
            rng_seed: None,
            sandbox_dir: None,
        }
    }
}

pub struct Interpreter<'a> {
    pub(crate) program: &'a Program,
    pub(crate) globals: ScopeFrame,
    pub(crate) frames: Vec<ScopeFrame>,
    pub(crate) routines: HashMap<String, &'a RoutineDecl>,
    pub(crate) resolved_types: HashMap<String, ResolvedType>,
    pub(crate) consts: HashMap<String, Value>,
    pub(crate) io: &'a mut dyn IoHandler,
    pub(crate) debug: &'a mut dyn DebugController,
    pub(crate) options: ExecOptions,
    pub(crate) rng: RngState,
    pub(crate) echo_enabled: bool,
    pub(crate) random_input: Option<(i64, i64)>,
    pub(crate) timer_ms: u64,
    pub(crate) stopwatch_start: Option<Instant>,
    pub(crate) file_mode: FileMode,
    pub(crate) allocated_cells: usize,
    pub(crate) block_depth: usize,
}

impl<'a> Interpreter<'a> {
    pub fn new(
        program: &'a Program,
        io: &'a mut dyn IoHandler,
        debug: &'a mut dyn DebugController,
        options: ExecOptions,
    ) -> Result<Self> {
        let rng = match options.rng_seed {
            Some(seed) => RngState::from_seed(seed),
            None => RngState::from_time(),
        };

        let mut allocated_cells: usize = 0;
        let mut globals = ScopeFrame::new("global");
        let mut consts = HashMap::new();

        for decl in &program.consts {
            let key = decl.name.to_lowercase();
            if consts.contains_key(&key) {
                return Err(AlgError::semantic(
                    decl.span,
                    format!("Constante '{}' declarada em duplicidade.", decl.name),
                ));
            }
            let val = eval_const_expr(&decl.value, &consts)?;
            let ty = val.resolved_type();
            let cells = ty.checked_leaf_count().unwrap_or(1);
            reserve_cells_in(&mut allocated_cells, cells, decl.span)?;
            consts.insert(key.clone(), val.clone());
            let slot = Rc::new(RefCell::new(VarSlot {
                root_name: decl.name.clone(),
                resolved_type: ty,
                is_const: true,
                value: val,
            }));
            globals.insert(
                key,
                SlotBinding::Slot {
                    display_name: decl.name.clone(),
                    slot,
                },
            );
        }

        let mut raw_types: HashMap<String, &TypeDecl> = HashMap::new();
        for tdecl in &program.types {
            let key = tdecl.name.to_lowercase();
            if raw_types.contains_key(&key) {
                return Err(AlgError::semantic(
                    tdecl.span,
                    format!("Tipo '{}' declarado em duplicidade.", tdecl.name),
                ));
            }
            raw_types.insert(key, tdecl);
        }

        let mut resolved_types = HashMap::new();
        let mut visiting = HashSet::new();
        for tdecl in &program.types {
            let key = tdecl.name.to_lowercase();
            if !resolved_types.contains_key(&key) {
                resolve_named_type(
                    &key,
                    tdecl.span,
                    &raw_types,
                    &mut resolved_types,
                    &consts,
                    &mut visiting,
                )?;
            }
        }

        for decl in &program.globals {
            let key = decl.name.to_lowercase();
            if globals.bindings.contains_key(&key) {
                return Err(AlgError::semantic(
                    decl.span,
                    format!(
                        "Identificador '{}' já foi declarado anteriormente.",
                        decl.name
                    ),
                ));
            }
            let rtype = resolve_var_type_spec(
                &decl.var_type,
                &raw_types,
                &mut resolved_types,
                &consts,
                &mut visiting,
                None,
            )?;
            let cells = rtype.checked_leaf_count().map_err(|_| {
                AlgError::runtime(
                    Some(decl.span),
                    format!("Variável '{}' excede o limite de memória da VM.", decl.name),
                )
            })?;
            reserve_cells_in(&mut allocated_cells, cells, decl.span)?;
            let val = Value::allocate_resolved(&decl.name, &rtype, decl.span)?;
            let slot = Rc::new(RefCell::new(VarSlot {
                root_name: decl.name.clone(),
                resolved_type: rtype,
                is_const: false,
                value: val,
            }));
            globals.insert(
                key,
                SlotBinding::Slot {
                    display_name: decl.name.clone(),
                    slot,
                },
            );
        }

        let mut routines = HashMap::new();
        for r in &program.routines {
            let key = r.name.to_lowercase();
            if routines.contains_key(&key) {
                return Err(AlgError::semantic(
                    r.span,
                    format!("Sub-rotina '{}' declarada em duplicidade.", r.name),
                ));
            }
            if is_builtin_name(&r.name) {
                return Err(AlgError::semantic(
                    r.span,
                    format!(
                        "Sub-rotina '{}' conflita com função interna da linguagem.",
                        r.name
                    ),
                ));
            }
            if globals.bindings.contains_key(&key) {
                return Err(AlgError::semantic(
                    r.span,
                    format!(
                        "Sub-rotina '{}' conflita com identificador global de mesmo nome.",
                        r.name
                    ),
                ));
            }

            let mut param_keys = HashSet::new();
            for p in &r.params {
                let pkey = p.name.to_lowercase();
                if !param_keys.insert(pkey) {
                    return Err(AlgError::semantic(
                        p.span,
                        format!(
                            "Parâmetro '{}' declarado em duplicidade na sub-rotina '{}'.",
                            p.name, r.name
                        ),
                    ));
                }
            }
            let mut local_keys = HashSet::new();
            for l in &r.locals {
                let lkey = l.name.to_lowercase();
                if param_keys.contains(&lkey) {
                    return Err(AlgError::semantic(
                        l.span,
                        format!(
                            "Variável local '{}' conflita com parâmetro de mesmo nome na sub-rotina '{}'.",
                            l.name, r.name
                        ),
                    ));
                }
                if !local_keys.insert(lkey) {
                    return Err(AlgError::semantic(
                        l.span,
                        format!(
                            "Variável local '{}' declarada em duplicidade na sub-rotina '{}'.",
                            l.name, r.name
                        ),
                    ));
                }
            }

            routines.insert(key, r);
        }

        Ok(Self {
            program,
            globals,
            frames: Vec::new(),
            routines,
            resolved_types,
            consts,
            io,
            debug,
            options,
            rng,
            echo_enabled: true,
            random_input: None,
            timer_ms: 0,
            stopwatch_start: None,
            file_mode: FileMode::None,
            allocated_cells,
            block_depth: 0,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        if let Some((path, span)) = &self.program.arquivo {
            activate_arquivo(
                self.options.sandbox_dir.as_deref(),
                &mut self.file_mode,
                path,
                *span,
            )?;
        }
        let res = self.exec_block(&self.program.body);
        flush_file_writer(&mut self.file_mode);
        match res.map_err(VmError::into_error)? {
            ExecSignal::Next => Ok(()),
            ExecSignal::Break(span) => Err(AlgError::runtime(
                Some(*span),
                "'interrompa' só pode ser usado dentro de um laço ('para', 'enquanto' ou 'repita').",
            )),
            ExecSignal::Return(ret) => Err(AlgError::runtime(
                Some(ret.1),
                "'retorne' só pode ser usado dentro de 'funcao' ou 'procedimento'.",
            )),
        }
    }

    pub fn snapshot_vars(&self) -> Vec<VarSnapshot> {
        snapshot_vars_from(&self.globals, &self.frames)
    }

    pub fn debug_snapshot(&self) -> DebugSnapshot {
        snapshot_debug_from(&self.program.title, &self.globals, &self.frames)
    }

    pub(crate) fn reserve_cells(&mut self, cells: usize, span: Span) -> Result<()> {
        reserve_cells_in(&mut self.allocated_cells, cells, span)
    }

    pub(crate) fn release_cells(&mut self, cells: usize) {
        self.allocated_cells = self.allocated_cells.saturating_sub(cells);
    }

    pub(crate) fn lookup_binding_opt(&self, name: &str) -> Option<SlotBinding> {
        let key = name.to_lowercase();
        if let Some(frame) = self.frames.last() {
            if let Some(binding) = frame.bindings.get(&key) {
                return Some(binding.clone());
            }
        }
        self.globals.bindings.get(&key).cloned()
    }

    pub(crate) fn lookup_binding(&self, name: &str, span: Span) -> Result<SlotBinding> {
        self.lookup_binding_opt(name).ok_or_else(|| {
            AlgError::runtime(
                Some(span),
                format!("Variável '{name}' não foi declarada."),
            )
        })
    }

    pub(crate) fn resolve_runtime_type(&mut self, vt: &VarType) -> Result<ResolvedType> {
        match vt {
            VarType::Primitive(p) => Ok(ResolvedType::Primitive(*p)),
            VarType::Named(name, span) => {
                let key = name.to_lowercase();
                self.resolved_types.get(&key).cloned().ok_or_else(|| {
                    AlgError::semantic(*span, format!("Tipo '{name}' não foi declarado."))
                })
            }
            VarType::Array { .. } | VarType::Record { .. } => self.resolve_runtime_type_complex(vt),
        }
    }

    #[inline(never)]
    fn resolve_runtime_type_complex(&mut self, vt: &VarType) -> Result<ResolvedType> {
        let raw_types = HashMap::new();
        let mut visiting = HashSet::new();
        resolve_var_type_spec(
            vt,
            &raw_types,
            &mut self.resolved_types,
            &self.consts,
            &mut visiting,
            None,
        )
    }
}

fn reserve_cells_in(allocated: &mut usize, cells: usize, span: Span) -> Result<()> {
    let next = allocated.checked_add(cells).ok_or_else(|| {
        AlgError::runtime(
            Some(span),
            format!(
                "Limite máximo de memória da VM ({MAX_VM_TOTAL_ELEMENTS} elementos) excedido."
            ),
        )
    })?;
    if next > MAX_VM_TOTAL_ELEMENTS {
        return Err(AlgError::runtime(
            Some(span),
            format!(
                "Limite máximo de memória da VM ({MAX_VM_TOTAL_ELEMENTS} elementos) excedido."
            ),
        ));
    }
    *allocated = next;
    Ok(())
}

pub fn parse_read_value(raw: &str, expected: PrimitiveType) -> std::result::Result<Value, String> {
    match expected {
        PrimitiveType::Inteiro => {
            let trimmed = raw.trim();
            trimmed
                .parse::<i64>()
                .map(Value::Inteiro)
                .map_err(|_| format!("'{trimmed}' não é um número inteiro válido."))
        }
        PrimitiveType::Real => {
            let trimmed = raw.trim();
            parse_finite_f64(trimmed)
                .map(Value::Real)
                .ok_or_else(|| format!("'{trimmed}' não é um número real válido."))
        }
        PrimitiveType::Caractere => {
            crate::value::ensure_string_len(raw.len(), None).map_err(|e| e.to_string())?;
            Ok(Value::Caractere(raw.to_string()))
        }
        PrimitiveType::Logico => match normalize_keyword(raw.trim()).as_str() {
            "verdadeiro" | "v" | "true" | "1" => Ok(Value::Logico(true)),
            "falso" | "f" | "false" | "0" => Ok(Value::Logico(false)),
            _ => Err(format!(
                "'{}' não é um valor lógico válido (use verdadeiro/falso).",
                raw.trim()
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compile, run_with_inputs, run_with_options};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_sandbox_dir(prefix: &str) -> PathBuf {
        let seq = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "algoritmoslab_vm_test_{prefix}_{}_{seq}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn live_var_aliasing_forwards_recursively_and_updates_snapshots() {
        let src = r#"algoritmo "Aliasing"
tipo
   TAluno = registro
      nome: caractere
      nota: real
   fimregistro
var
   v: vetor[1..3] de inteiro
   turma: vetor[1..2] de TAluno

procedimento p3(var b: inteiro)
inicio
   b <- b + 5
fimprocedimento

procedimento p2(var a: inteiro)
inicio
   v[1] <- 20
   escreval("em_p2:", a)
   p3(a)
fimprocedimento

procedimento p1(var x: inteiro)
inicio
   p2(x)
fimprocedimento

procedimento muda_nota(var n: real)
inicio
   n <- 9.5
fimprocedimento

procedimento encaminha_aluno(var al: TAluno)
inicio
   muda_nota(al.nota)
fimprocedimento

procedimento troca(var a, b: inteiro)
var
   aux: inteiro
inicio
   aux <- a
   a <- b
   b <- aux
fimprocedimento

inicio
   v[1] <- 10
   p1(v[1])
   escreval("v1:", v[1])
   troca(v[1], v[1])
   escreval("troca_mesmo:", v[1])
   turma[1].nota <- 5.0
   encaminha_aluno(turma[1])
   escreval("nota:", turma[1].nota:3:1)
fimalgoritmo
"#;
        let out = run_with_inputs(src, Vec::<String>::new()).unwrap();
        assert_eq!(out, "em_p2:20\nv1:25\ntroca_mesmo:25\nnota:9.5\n");
    }

    #[test]
    fn by_ref_blocks_const_and_mismatched_types_at_entry() {
        let const_src = r#"algoritmo "ConstRef"
const
   C = 10
procedimento p(var x: inteiro)
inicio
   x <- 1
fimprocedimento
inicio
   p(C)
fimalgoritmo
"#;
        assert!(run_with_inputs(const_src, Vec::<String>::new()).is_err());

        let mismatch_src = r#"algoritmo "MismatchRef"
var
   v: vetor[1..2] de inteiro
procedimento p(var r: real)
inicio
fimprocedimento
inicio
   p(v[1])
fimalgoritmo
"#;
        assert!(run_with_inputs(mismatch_src, Vec::<String>::new()).is_err());
    }

    #[test]
    fn callee_snapshot_shows_param_display_name_without_key_collision() {
        let src = r#"algoritmo "SnapParams"
var
   v: vetor[1..2] de inteiro
procedimento dup(var a, b: inteiro)
inicio
   a <- 7
   b <- 9
fimprocedimento
inicio
   dup(v[1], v[1])
fimalgoritmo
"#;
        struct SnapDbg(Vec<VarSnapshot>);
        impl DebugController for SnapDbg {
            fn check_cancel(&self) -> Result<()> {
                Ok(())
            }
            fn before_statement(
                &mut self,
                line: usize,
                _call_depth: usize,
                snapshot: &mut dyn FnMut() -> DebugSnapshot,
            ) -> Result<()> {
                if line == 7 {
                    self.0 = snapshot().variables;
                }
                Ok(())
            }
        }
        let prog = compile(src).unwrap();
        let mut io = MemoryIo::new(Vec::<String>::new());
        let mut dbg = SnapDbg(Vec::new());
        let mut vm = Interpreter::new(&prog, &mut io, &mut dbg, ExecOptions::default()).unwrap();
        vm.run().unwrap();
        assert!(dbg.0.iter().any(|v| v.scope == "dup" && v.name == "a" && v.value_text == "7"));
        assert!(dbg.0.iter().any(|v| v.scope == "dup" && v.name == "b" && v.value_text == "7"));
    }

    #[test]
    fn preserves_failing_frame_and_locals_on_runtime_error() {
        let src = r#"algoritmo "FailSnap"
procedimento falhar(arg: inteiro)
var
   local_x: inteiro
inicio
   local_x <- arg * 2
   escreval(local_x / 0)
fimprocedimento
inicio
   falhar(21)
fimalgoritmo
"#;
        let prog = compile(src).unwrap();
        let mut io = MemoryIo::new(Vec::<String>::new());
        let mut dbg = RunController::noop();
        let mut vm = Interpreter::new(&prog, &mut io, &mut dbg, ExecOptions::default()).unwrap();
        assert!(vm.run().is_err());
        let snap = vm.debug_snapshot();
        assert_eq!(snap.call_stack, vec!["algoritmo \"FailSnap\"", "falhar"]);
        assert!(snap
            .variables
            .iter()
            .any(|v| v.scope == "falhar" && v.name == "local_x" && v.value_text == "42"));
    }

    #[test]
    fn for_loop_handles_overflow_and_real_steps_cleanly() {
        let src = r#"algoritmo "ForRealAndMax"
var
   i: inteiro
   r: real
inicio
   para i de 1 ate 10 / 2 passo 2 faca
      escreva(i, " ")
   fimpara
   escreval
   para r de 0.5 ate 1.5 passo 0.5 faca
      escreva(r, " ")
   fimpara
   escreval
   para i de 9223372036854775806 ate 9223372036854775807 faca
      escreva(i, " ")
   fimpara
fimalgoritmo
"#;
        let out = run_with_inputs(src, Vec::<String>::new()).unwrap();
        assert_eq!(
            out,
            "1 3 5 \n0.5 1 1.5 \n9223372036854775806 9223372036854775807 "
        );
    }

    #[test]
    fn enforces_vm_total_cell_budget_across_recursion() {
        let src = r#"algoritmo "Budget"
tipo
   TBig = vetor[1..500000] de inteiro
var
   g: TBig
procedimento rec(n: inteiro; v: TBig)
inicio
   se n > 0 entao
      rec(n - 1, v)
   fimse
fimprocedimento
inicio
   rec(10, g)
fimalgoritmo
"#;
        let err = run_with_inputs(src, Vec::<String>::new()).unwrap_err();
        assert!(err.message.contains("memória da VM"));
    }

    #[test]
    fn sandbox_rejects_reserved_names_symlinks_and_lazy_creates_files() {
        let dir = temp_sandbox_dir("sandbox");

        // Windows reserved device name rejected
        let bad_con = "algoritmo \"con\"\narquivo \"CON.txt\"\nvar s: caractere\ninicio\nleia(s)\nfimalgoritmo\n";
        let err_con = run_with_options(
            bad_con,
            vec!["a"],
            ExecOptions {
                sandbox_dir: Some(dir.clone()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err_con.to_string().contains("Acesso negado"));

        // Lazy creation: without leia, file is not created
        let no_read = "algoritmo \"lazy\"\narquivo \"nao_criado.txt\"\ninicio\nescreval(\"ok\")\nfimalgoritmo\n";
        let out = run_with_options(
            no_read,
            Vec::<String>::new(),
            ExecOptions {
                sandbox_dir: Some(dir.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(out, "ok\n");
        assert!(!dir.join("nao_criado.txt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn echoes_input_only_after_valid_parse() {
        struct ReReadIo {
            inputs: VecDeque<String>,
            errors: Vec<String>,
            echoed: Vec<String>,
        }
        impl IoHandler for ReReadIo {
            fn write(&mut self, _t: &str) {}
            fn clear(&mut self) {}
            fn write_error(&mut self, t: &str) {
                self.errors.push(t.to_string());
            }
            fn echo_input(&mut self, t: &str) {
                self.echoed.push(t.to_string());
            }
            fn read_input(&mut self, _v: &str, _t: &str, _l: usize) -> Result<String> {
                Ok(self.inputs.pop_front().unwrap())
            }
        }
        let src = "algoritmo \"e\"\nvar x: inteiro\ninicio\nleia(x)\nfimalgoritmo\n";
        let prog = compile(src).unwrap();
        let mut io = ReReadIo {
            inputs: VecDeque::from(vec!["oops".to_string(), "42".to_string()]),
            errors: Vec::new(),
            echoed: Vec::new(),
        };
        let mut dbg = RunController::noop();
        let mut vm = Interpreter::new(&prog, &mut io, &mut dbg, ExecOptions::default()).unwrap();
        vm.run().unwrap();
        assert_eq!(io.errors.len(), 1);
        assert_eq!(io.echoed, vec!["42".to_string()]);
    }

    #[test]
    fn disallows_rand_in_const_and_forbids_shadowing_or_collisions() {
        let rand_const = "algoritmo \"rc\"\nconst X = randi(10)\ninicio\nfimalgoritmo\n";
        assert!(run_with_inputs(rand_const, Vec::<String>::new()).is_err());

        let shadow_local = "algoritmo \"sl\"\nprocedimento p(x: inteiro)\nvar x: inteiro\ninicio\nfimprocedimento\ninicio\nfimalgoritmo\n";
        assert!(run_with_inputs(shadow_local, Vec::<String>::new()).is_err());

        let builtin_collision = "algoritmo \"bc\"\nfuncao abs(x: inteiro): inteiro\ninicio\nretorne x\nfimfuncao\ninicio\nfimalgoritmo\n";
        assert!(run_with_inputs(builtin_collision, Vec::<String>::new()).is_err());

        let global_collision = "algoritmo \"gc\"\nvar foo: inteiro\nprocedimento foo\ninicio\nfimprocedimento\ninicio\nfimalgoritmo\n";
        assert!(run_with_inputs(global_collision, Vec::<String>::new()).is_err());
    }

    #[test]
    fn parse_read_value_rejects_nan_and_inf() {
        assert!(parse_read_value("NaN", PrimitiveType::Real).is_err());
        assert!(parse_read_value("inf", PrimitiveType::Real).is_err());
        assert!(parse_read_value("-infinity", PrimitiveType::Real).is_err());
        assert_eq!(
            parse_read_value("3,25", PrimitiveType::Real).unwrap(),
            Value::Real(3.25)
        );
    }

    #[test]
    fn checks_diagnostic_messages_for_proc_builtin_and_string_index() {
        let err_proc = run_with_inputs("algoritmo \"p\"\ninicio\nnaoexiste\nfimalgoritmo\n", Vec::<String>::new())
            .unwrap_err();
        assert!(err_proc.message.contains("não declarado."));

        let err_builtin_stmt =
            run_with_inputs("algoritmo \"b\"\ninicio\nraizq(9)\nfimalgoritmo\n", Vec::<String>::new())
                .unwrap_err();
        assert!(err_builtin_stmt.message.contains("expressão"));

        let err_str_idx = run_with_inputs(
            "algoritmo \"s\"\nvar s: caractere\ninicio\ns <- \"oi\"\nescreval(s[1])\nfimalgoritmo\n",
            Vec::<String>::new(),
        )
        .unwrap_err();
        assert!(err_str_idx.message.contains("copia("));
    }

    #[test]
    fn rejects_real_overflow_to_infinity() {
        let src = "algoritmo \"inf\"\nvar x: real\ninicio\nx <- (10 ^ 308) * 10\nfimalgoritmo\n";
        let err = run_with_inputs(src, Vec::<String>::new()).unwrap_err();
        assert!(
            err.message.contains("não finito"),
            "mensagem inesperada: {}",
            err.message
        );

        let div_ov = "algoritmo \"infdiv\"\nvar x: real\ninicio\nx <- (10 ^ 308) / (10 ^ -308)\nfimalgoritmo\n";
        let err_div = run_with_inputs(div_ov, Vec::<String>::new()).unwrap_err();
        assert!(
            err_div.message.contains("não finito"),
            "mensagem inesperada: {}",
            err_div.message
        );
    }

    #[test]
    fn user_routine_evaluates_side_effect_arguments_exactly_once() {
        let src = r#"algoritmo "ArgOnce"
var
   c: inteiro
funcao inc(): inteiro
inicio
   c <- c + 1
   retorne c
fimfuncao
funcao dobro(x: inteiro): inteiro
inicio
   retorne x * 2
fimfuncao
procedimento consome(y: inteiro)
inicio
   escreval("p:", y)
fimprocedimento
inicio
   c <- 0
   escreval("f:", dobro(inc()))
   consome(inc())
   escreval("c:", c)
fimalgoritmo
"#;
        let out = run_with_inputs(src, Vec::<String>::new()).unwrap();
        assert_eq!(out, "f:2\np:2\nc:2\n");

        // Builtin usada como comando deve falhar sem avaliar argumentos com efeito colateral.
        let builtin_stmt_src = r#"algoritmo "BuiltinStmtNoEval"
var
   c: inteiro
funcao inc(): inteiro
inicio
   c <- c + 1
   retorne c
fimfuncao
inicio
   c <- 0
   raizq(inc())
fimalgoritmo
"#;
        let prog = compile(builtin_stmt_src).unwrap();
        let mut io = MemoryIo::new(Vec::<String>::new());
        let mut dbg = RunController::noop();
        let mut vm = Interpreter::new(&prog, &mut io, &mut dbg, ExecOptions::default()).unwrap();
        let err = vm.run().unwrap_err();
        assert!(err.message.contains("expressão"));
        assert!(vm
            .snapshot_vars()
            .iter()
            .any(|v| v.name == "c" && v.value_text == "0"));
    }

    #[test]
    fn enforces_max_string_bytes_in_concatenation_and_write() {
        // 24 duplicações de "a" -> 16 MiB exatos (ainda válido); 25ª excede MAX_STRING_BYTES.
        let concat_err_src = r#"algoritmo "StrMaxConcat"
var
   s: caractere
   i: inteiro
inicio
   s <- "a"
   para i de 1 ate 25 faca
      s <- s + s
   fimpara
fimalgoritmo
"#;
        let err_concat = run_with_inputs(concat_err_src, Vec::<String>::new()).unwrap_err();
        assert!(
            err_concat.message.contains("Tamanho máximo de texto"),
            "mensagem inesperada: {}",
            err_concat.message
        );

        // Em escreva, dois argumentos de 16 MiB (2^24) ultrapassam o limite no buffer de saída.
        let write_err_src = r#"algoritmo "StrMaxWrite"
var
   s: caractere
   i: inteiro
inicio
   s <- "a"
   para i de 1 ate 24 faca
      s <- s + s
   fimpara
   escreva(s, "x")
fimalgoritmo
"#;
        let err_write = run_with_inputs(write_err_src, Vec::<String>::new()).unwrap_err();
        assert!(
            err_write.message.contains("Tamanho máximo de texto"),
            "mensagem inesperada: {}",
            err_write.message
        );
    }

    #[test]
    fn allows_500_simple_recursive_calls() {
        let ok_500_src = r#"algoritmo "Rec500"
funcao soma(n: inteiro): inteiro
inicio
   se n <= 0 entao
      retorne 0
   fimse
   retorne n + soma(n - 1)
fimfuncao
inicio
   escreval(soma(500))
fimalgoritmo
"#;
        let out = run_with_options(
            ok_500_src,
            Vec::<String>::new(),
            ExecOptions {
                max_call_depth: 600,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(out, "125250\n");
    }

    #[test]
    fn enforces_combined_block_depth_without_stack_overflow() {
        // Combina 60 níveis de `se` aninhados + recursão entrando em `se` (2 blocos/chamada),
        // ultrapassando MAX_EXEC_BLOCK_DEPTH (768) sem stack overflow.
        let mut combined_src = String::from(
            r#"algoritmo "BlockDepthOverflow"
funcao rec(n: inteiro): inteiro
inicio
   se n > 0 entao
      retorne rec(n - 1)
   fimse
   retorne 0
fimfuncao
inicio
"#,
        );
        for _ in 0..60 {
            combined_src.push_str("   se verdadeiro entao\n");
        }
        combined_src.push_str("      escreval(rec(400))\n");
        for _ in 0..60 {
            combined_src.push_str("   fimse\n");
        }
        combined_src.push_str("fimalgoritmo\n");

        let err = run_with_options(
            &combined_src,
            Vec::<String>::new(),
            ExecOptions {
                max_call_depth: 1000,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            err.message.contains("profundidade de execução aninhada"),
            "mensagem inesperada: {}",
            err.message
        );
    }

    #[test]
    fn nested_builtin_calls_respect_max_eval_depth() {
        // Verifica propagação de profundidade em builtins aninhadas via AST acima de MAX_EVAL_DEPTH (256).
        let span = Span::new(1, 1, 1);
        let mut expr = Expr::Int(1, span);
        for _ in 0..140 {
            expr = Expr::Call {
                name: "abs".to_string(),
                args: vec![expr],
                span,
            };
        }
        let prog = Program {
            title: "DeepBuiltinAst".to_string(),
            consts: Vec::new(),
            types: Vec::new(),
            arquivo: None,
            globals: vec![VarDecl {
                name: "x".to_string(),
                var_type: VarType::Primitive(PrimitiveType::Inteiro),
                span,
            }],
            routines: Vec::new(),
            body: vec![Stmt::Assign {
                target: LValue {
                    base: "x".to_string(),
                    base_span: span,
                    steps: Vec::new(),
                    span,
                },
                value: expr,
                span,
            }],
            end_line: 1,
        };
        let mut io = MemoryIo::new(Vec::<String>::new());
        let mut dbg = RunController::noop();
        let mut vm = Interpreter::new(&prog, &mut io, &mut dbg, ExecOptions::default()).unwrap();
        let err = vm.run().unwrap_err();
        assert!(
            err.message.contains("Profundidade máxima de avaliação"),
            "mensagem inesperada: {}",
            err.message
        );

        let nested_ok = format!(
            "algoritmo \"OkBuiltin\"\nvar x: inteiro\ninicio\n   x <- {}1{}\n   escreval(x)\nfimalgoritmo\n",
            "abs(".repeat(80),
            ")".repeat(80)
        );
        let out = run_with_inputs(&nested_ok, Vec::<String>::new()).unwrap();
        assert_eq!(out, "1\n");
    }

    #[test]
    fn supports_sequential_arquivo_switching_inside_inicio() {
        let dir = temp_sandbox_dir("seq_arquivo");
        let src = r#"algoritmo "SeqArquivo"
var
   nome1, nome2: caractere
   n1, n2: inteiro
inicio
   arquivo "primeiro.txt"
   leia(nome1, n1)
   arquivo "primeiro.txt"
   leia(nome2, n2)
   arquivo "segundo.txt"
   leia(nome1)
   escreval(nome2, ":", n2, ":", nome1)
fimalgoritmo
"#;
        let out = run_with_options(
            src,
            vec!["Maria", "42", "Extra"],
            ExecOptions {
                sandbox_dir: Some(dir.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(out, "Maria:42:Extra\n");
        let first_contents = std::fs::read_to_string(dir.join("primeiro.txt")).unwrap();
        assert_eq!(first_contents, "Maria\n42\n");
        let second_contents = std::fs::read_to_string(dir.join("segundo.txt")).unwrap();
        assert_eq!(second_contents, "Extra\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_deep_acyclic_type_alias_chain_exceeding_max_depth() {
        let mut ok_src = String::from("algoritmo \"DeepTypeOk\"\ntipo\n");
        for i in 0..place::MAX_TYPE_RESOLUTION_DEPTH - 1 {
            ok_src.push_str(&format!("   T{i} = T{}\n", i + 1));
        }
        ok_src.push_str(&format!(
            "   T{} = inteiro\nvar\n   x: T0\ninicio\n   x <- 7\n   escreval(x)\nfimalgoritmo\n",
            place::MAX_TYPE_RESOLUTION_DEPTH - 1
        ));
        let ok_out = run_with_inputs(&ok_src, Vec::<String>::new()).unwrap();
        assert_eq!(ok_out, "7\n");

        let mut err_src = String::from("algoritmo \"DeepTypeErr\"\ntipo\n");
        for i in 0..place::MAX_TYPE_RESOLUTION_DEPTH {
            err_src.push_str(&format!("   T{i} = T{}\n", i + 1));
        }
        err_src.push_str(&format!(
            "   T{} = inteiro\nvar\n   x: T0\ninicio\nfimalgoritmo\n",
            place::MAX_TYPE_RESOLUTION_DEPTH
        ));
        let err = run_with_inputs(&err_src, Vec::<String>::new()).unwrap_err();
        assert_eq!(err.kind, crate::ErrorKind::Semantico);
        assert!(
            err.message
                .contains("Profundidade máxima de resolução de tipos"),
            "mensagem inesperada: {}",
            err.message
        );
    }

    #[test]
    fn parse_read_value_enforces_max_string_bytes() {
        let at_limit = "a".repeat(crate::value::MAX_STRING_BYTES);
        assert!(parse_read_value(&at_limit, PrimitiveType::Caractere).is_ok());

        let over_limit = "a".repeat(crate::value::MAX_STRING_BYTES + 1);
        let err = parse_read_value(&over_limit, PrimitiveType::Caractere).unwrap_err();
        assert!(
            err.contains("Tamanho máximo de texto"),
            "mensagem inesperada: {err}"
        );
    }

    #[test]
    fn record_and_array_preview_truncate_long_strings_safely() {
        use crate::value::{
            ArrayValue, RecordFieldSpec, RecordTypeSpec, RecordValue, ResolvedArrayRange,
        };
        use std::sync::Arc;

        let inner_spec = Arc::new(RecordTypeSpec {
            type_name: Some("TInner".to_string()),
            fields: vec![RecordFieldSpec {
                key: "etiqueta".to_string(),
                name: "etiqueta".to_string(),
                ty: ResolvedType::Primitive(PrimitiveType::Caractere),
            }],
        });
        let outer_spec = Arc::new(RecordTypeSpec {
            type_name: Some("TOuter".to_string()),
            fields: vec![
                RecordFieldSpec {
                    key: "texto".to_string(),
                    name: "texto".to_string(),
                    ty: ResolvedType::Primitive(PrimitiveType::Caractere),
                },
                RecordFieldSpec {
                    key: "exato".to_string(),
                    name: "exato".to_string(),
                    ty: ResolvedType::Primitive(PrimitiveType::Caractere),
                },
                RecordFieldSpec {
                    key: "aninhado".to_string(),
                    name: "aninhado".to_string(),
                    ty: ResolvedType::Record(Arc::clone(&inner_spec)),
                },
            ],
        });

        let inner_rec = RecordValue {
            spec: inner_spec,
            fields: vec![Value::Caractere("curto".to_string())],
        };
        let outer_rec = RecordValue {
            spec: outer_spec,
            fields: vec![
                Value::Caractere("é".repeat(200)),
                Value::Caractere("b".repeat(120)),
                Value::Record(Box::new(inner_rec)),
            ],
        };

        let rec_preview = outer_rec.preview();
        let expected_truncated = format!("{}...", "é".repeat(120));
        let expected_exact = "b".repeat(120);
        assert!(rec_preview.contains(&expected_truncated));
        assert!(!rec_preview.contains(&"é".repeat(121)));
        assert!(rec_preview.contains(&expected_exact));
        assert!(rec_preview.contains("{etiqueta: curto}"));

        let arr = ArrayValue {
            elem_type: ResolvedType::Primitive(PrimitiveType::Caractere),
            dims: vec![ResolvedArrayRange { start: 1, end: 2 }],
            elements: vec![
                Value::Caractere("c".repeat(500)),
                Value::Caractere("ok".to_string()),
            ],
        };
        let arr_preview = arr.preview();
        assert_eq!(
            arr_preview,
            format!("[{}..., ok]", "c".repeat(120))
        );
    }

    #[test]
    fn sequential_arquivo_switching_inside_procedimento_flushes_and_switches() {
        let dir = temp_sandbox_dir("proc_seq_arquivo");
        let src = r#"algoritmo "ProcSeqArquivo"
var
   lido_primeiro, lido_segundo: caractere
   num_primeiro: inteiro

procedimento gravar_e_trocar()
var
   a, b: caractere
   n: inteiro
inicio
   arquivo "proc_primeiro.txt"
   leia(a, n)
   arquivo "proc_primeiro.txt"
   leia(lido_primeiro, num_primeiro)
   arquivo "proc_segundo.txt"
   leia(b)
   lido_segundo <- b
fimprocedimento

inicio
   gravar_e_trocar()
   escreval(lido_primeiro, ":", num_primeiro, ":", lido_segundo)
fimalgoritmo
"#;
        let out = run_with_options(
            src,
            vec!["Ana", "15", "Beta"],
            ExecOptions {
                sandbox_dir: Some(dir.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(out, "Ana:15:Beta\n");
        let first_contents = std::fs::read_to_string(dir.join("proc_primeiro.txt")).unwrap();
        assert_eq!(first_contents, "Ana\n15\n");
        let second_contents = std::fs::read_to_string(dir.join("proc_segundo.txt")).unwrap();
        assert_eq!(second_contents, "Beta\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
