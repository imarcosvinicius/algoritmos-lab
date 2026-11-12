use super::place::{index_type_error, read_at_path};
use super::{vm_runtime_err, Interpreter, VmResult};
use crate::ast::{BinaryOp, Expr, UnaryOp};
use crate::builtins::{call_builtin, ensure_finite, eval_pow, is_zero_arg_builtin, RngState};
use crate::error::{Result, Span, AlgError};
use crate::lexer::normalize_keyword;
use crate::value::{ensure_string_len, Value};
use std::collections::HashMap;

pub(crate) const MAX_EVAL_DEPTH: usize = 256;
const MAX_EVAL_WORK_STEPS: usize = 131_072;

enum EvalTask<'e> {
    Expr(&'e Expr),
    ApplyUnary(UnaryOp, Span),
    ApplyBinary(BinaryOp, Span),
    ShortCircuitRhs(BinaryOp, &'e Expr, &'e Expr),
    ExpectBool(&'e Expr),
    ApplyField(&'e str, Span),
    ApplyIndex(&'e [Expr], Span),
}

enum ConstTask<'e> {
    Expr(&'e Expr),
    ApplyUnary(UnaryOp, Span),
    ApplyBinary(BinaryOp, Span),
    ShortCircuitRhs(BinaryOp, &'e Expr, &'e Expr),
    ExpectBool(&'e Expr),
    ApplyCall(&'e str, usize, Span),
}

fn pop_two_values(vals: &mut Vec<Value>, span: Span) -> Result<(Value, Value)> {
    let r = vals
        .pop()
        .ok_or_else(|| AlgError::runtime(Some(span), "Pilha de avaliação inconsistente."))?;
    let l = vals
        .pop()
        .ok_or_else(|| AlgError::runtime(Some(span), "Pilha de avaliação inconsistente."))?;
    Ok((l, r))
}

impl<'a> Interpreter<'a> {
    pub(crate) fn eval_expr(&mut self, expr: &Expr) -> VmResult<Value> {
        self.eval_expr_depth(expr, 0)
    }

    #[inline(never)]
    fn eval_var_expr(&mut self, name: &str, span: Span, depth: usize) -> VmResult<Value> {
        if let Some(binding) = self.lookup_binding_opt(name) {
            return Ok(binding.read_value()?);
        }
        if is_zero_arg_builtin(name) {
            if let Some(val) = call_builtin(name, Vec::new(), span, &mut self.rng)? {
                return Ok(val);
            }
        }
        let key = name.to_lowercase();
        if let Some(r) = self.routines.get(&key) {
            if r.return_type.is_none() {
                return Err(vm_runtime_err(
                    Some(span),
                    format!(
                        "'{}' é um procedimento e não pode ser usado como expressão.",
                        r.name
                    ),
                ));
            }
            if !r.params.is_empty() {
                return Err(vm_runtime_err(
                    Some(span),
                    format!(
                        "'{}' esperava {} parâmetro(s), mas recebeu 0.",
                        r.name,
                        r.params.len()
                    ),
                ));
            }
            return self
                .invoke_routine_depth(name, &[], span, true, depth.saturating_add(1))?
                .ok_or_else(|| vm_runtime_err(Some(span), "Função não retornou valor."));
        }
        Err(vm_runtime_err(
            Some(span),
            format!("Variável ou função '{name}' não declarada."),
        ))
    }

    #[inline(never)]
    fn eval_lvalue_read(&mut self, cur: &Expr, depth: usize) -> VmResult<Option<Value>> {
        if let Some(lval) = super::place::expr_to_lvalue(cur) {
            if !lval.steps.is_empty() {
                let place = self.resolve_place_depth(&lval, depth + 1)?;
                let borrowed = place.slot.borrow();
                let val = read_at_path(&borrowed.value, &place.base_name, &place.steps)?.clone();
                return Ok(Some(val));
            }
        }
        Ok(None)
    }

    #[inline(never)]
    fn eval_non_call_expr_task<'e>(
        &mut self,
        cur: &'e Expr,
        work: &mut Vec<EvalTask<'e>>,
        vals: &mut Vec<Value>,
        depth: usize,
    ) -> VmResult<()> {
        if matches!(cur, Expr::Field { .. } | Expr::Index { .. }) {
            if let Some(val) = self.eval_lvalue_read(cur, depth)? {
                vals.push(val);
                return Ok(());
            }
        }
        if let Expr::Var(name, span) = cur {
            vals.push(self.eval_var_expr(name, *span, depth)?);
            return Ok(());
        }
        push_non_call_expr(cur, work, vals);
        Ok(())
    }

    #[inline(never)]
    fn run_non_call_task<'e>(
        &mut self,
        task: EvalTask<'e>,
        work: &mut Vec<EvalTask<'e>>,
        vals: &mut Vec<Value>,
        depth: usize,
    ) -> VmResult<()> {
        match task {
            EvalTask::Expr(cur) => self.eval_non_call_expr_task(cur, work, vals, depth),
            other => Ok(eval_other_task(other, work, vals)?),
        }
    }

    #[inline(never)]
    fn eval_call_task(
        &mut self,
        name: &str,
        args: &[Expr],
        span: Span,
        depth: usize,
        vals: &mut Vec<Value>,
    ) -> VmResult<()> {
        let val = self
            .invoke_routine_depth(name, args, span, true, depth.saturating_add(1))?
            .ok_or_else(|| eval_no_return_err(span))?;
        vals.push(val);
        Ok(())
    }

    #[inline(never)]
    fn eval_expr_complex(&mut self, expr: &Expr, depth: usize) -> VmResult<Value> {
        let mut work = Vec::with_capacity(8);
        work.push(EvalTask::Expr(expr));
        let mut vals: Vec<Value> = Vec::with_capacity(8);
        let mut steps = 0usize;

        while let Some(task) = work.pop() {
            steps += 1;
            if steps > MAX_EVAL_WORK_STEPS {
                return Err(eval_steps_exceeded_err(expr.span()));
            }
            match task {
                EvalTask::Expr(Expr::Call { name, args, span }) => {
                    self.eval_call_task(name, args, *span, depth, &mut vals)?;
                }
                other => self.run_non_call_task(other, &mut work, &mut vals, depth)?,
            }
        }

        vals.pop().ok_or_else(|| eval_no_result_err(expr.span()))
    }

    #[inline(never)]
    pub(crate) fn eval_expr_depth(&mut self, expr: &Expr, depth: usize) -> VmResult<Value> {
        if depth > MAX_EVAL_DEPTH {
            return Err(eval_depth_exceeded_err(expr.span()));
        }
        if let Expr::Call { name, args, span } = expr {
            return self
                .invoke_routine_depth(name, args, *span, true, depth.saturating_add(1))?
                .ok_or_else(|| eval_no_return_err(*span));
        }
        self.eval_expr_complex(expr, depth)
    }

    pub(crate) fn eval_indices_depth(
        &mut self,
        indices: &[Expr],
        depth: usize,
    ) -> VmResult<Vec<i64>> {
        let mut out = Vec::with_capacity(indices.len());
        for idx_expr in indices {
            let v = self.eval_expr_depth(idx_expr, depth)?;
            out.push(v.as_int(idx_expr.span())?);
        }
        Ok(out)
    }
}

#[inline(never)]
fn push_non_call_expr<'e>(cur: &'e Expr, work: &mut Vec<EvalTask<'e>>, vals: &mut Vec<Value>) {
    match cur {
        Expr::Int(n, _) => vals.push(Value::Inteiro(*n)),
        Expr::Real(f, _) => vals.push(Value::Real(*f)),
        Expr::Str(s, _) => vals.push(Value::Caractere(s.clone())),
        Expr::Bool(b, _) => vals.push(Value::Logico(*b)),
        Expr::Field {
            target,
            field,
            span,
        } => {
            work.push(EvalTask::ApplyField(field, *span));
            work.push(EvalTask::Expr(target));
        }
        Expr::Index {
            target,
            indices,
            span,
        } => {
            work.push(EvalTask::ApplyIndex(indices, *span));
            work.push(EvalTask::Expr(target));
            for idx_expr in indices.iter().rev() {
                work.push(EvalTask::Expr(idx_expr));
            }
        }
        Expr::Unary { op, expr, span } => {
            work.push(EvalTask::ApplyUnary(*op, *span));
            work.push(EvalTask::Expr(expr));
        }
        Expr::Binary {
            left,
            op,
            right,
            span,
        } => {
            if matches!(op, BinaryOp::And | BinaryOp::Or) {
                work.push(EvalTask::ShortCircuitRhs(*op, left, right));
                work.push(EvalTask::Expr(left));
            } else {
                work.push(EvalTask::ApplyBinary(*op, *span));
                work.push(EvalTask::Expr(right));
                work.push(EvalTask::Expr(left));
            }
        }
        Expr::Var(_, _) | Expr::Call { .. } => {}
    }
}

#[cold]
#[inline(never)]
fn eval_depth_exceeded_err(span: Span) -> super::VmError {
    vm_runtime_err(
        Some(span),
        format!("Profundidade máxima de avaliação de expressão ({MAX_EVAL_DEPTH}) excedida."),
    )
}

#[cold]
#[inline(never)]
fn eval_steps_exceeded_err(span: Span) -> super::VmError {
    vm_runtime_err(
        Some(span),
        "Complexidade máxima de avaliação de expressão excedida.",
    )
}

#[cold]
#[inline(never)]
fn eval_no_return_err(span: Span) -> super::VmError {
    vm_runtime_err(Some(span), "Função não retornou valor.")
}

#[cold]
#[inline(never)]
fn eval_no_result_err(span: Span) -> super::VmError {
    vm_runtime_err(Some(span), "Expressão sem resultado.")
}

#[inline(never)]
fn eval_other_task<'e>(
    task: EvalTask<'e>,
    work: &mut Vec<EvalTask<'e>>,
    vals: &mut Vec<Value>,
) -> Result<()> {
    match task {
        EvalTask::ApplyUnary(op, span) => apply_unary_task(vals, op, span),
        EvalTask::ApplyBinary(op, span) => apply_binary_task(vals, op, span),
        EvalTask::ShortCircuitRhs(op, left, right) => {
            apply_short_circuit_task(work, vals, op, left, right)
        }
        EvalTask::ExpectBool(e) => apply_expect_bool_task(vals, e.span()),
        EvalTask::ApplyField(field, span) => apply_field_task(vals, field, span),
        EvalTask::ApplyIndex(indices, span) => apply_index_task(vals, indices, span),
        EvalTask::Expr(_) => Ok(()),
    }
}

#[inline(never)]
fn apply_unary_task(vals: &mut Vec<Value>, op: UnaryOp, span: Span) -> Result<()> {
    let val = vals
        .pop()
        .ok_or_else(|| AlgError::runtime(Some(span), "Pilha de avaliação inconsistente."))?;
    vals.push(eval_unary(op, val, span)?);
    Ok(())
}

#[inline(never)]
fn apply_binary_task(vals: &mut Vec<Value>, op: BinaryOp, span: Span) -> Result<()> {
    let (l, r) = pop_two_values(vals, span)?;
    vals.push(eval_binary(l, op, r, span)?);
    Ok(())
}

#[inline(never)]
fn apply_short_circuit_task<'e>(
    work: &mut Vec<EvalTask<'e>>,
    vals: &mut Vec<Value>,
    op: BinaryOp,
    left: &'e Expr,
    right: &'e Expr,
) -> Result<()> {
    let left_span = left.span();
    let lv = vals
        .pop()
        .ok_or_else(|| AlgError::runtime(Some(left_span), "Pilha de avaliação inconsistente."))?
        .as_bool(left_span)?;
    match (op, lv) {
        (BinaryOp::And, false) => vals.push(Value::Logico(false)),
        (BinaryOp::Or, true) => vals.push(Value::Logico(true)),
        _ => {
            work.push(EvalTask::ExpectBool(right));
            work.push(EvalTask::Expr(right));
        }
    }
    Ok(())
}

#[inline(never)]
fn apply_expect_bool_task(vals: &mut Vec<Value>, span: Span) -> Result<()> {
    let rv = vals
        .pop()
        .ok_or_else(|| AlgError::runtime(Some(span), "Pilha de avaliação inconsistente."))?
        .as_bool(span)?;
    vals.push(Value::Logico(rv));
    Ok(())
}

#[inline(never)]
fn apply_field_task(vals: &mut Vec<Value>, field: &str, span: Span) -> Result<()> {
    let base_val = vals
        .pop()
        .ok_or_else(|| AlgError::runtime(Some(span), "Pilha de avaliação inconsistente."))?;
    match base_val {
        Value::Record(rec) => vals.push(rec.get_field(field, span)?.clone()),
        other => {
            return Err(AlgError::runtime(
                Some(span),
                format!(
                    "Acesso ao campo '.{field}' inválido: esperado registro, recebeu '{}'.",
                    other.type_label()
                ),
            ));
        }
    }
    Ok(())
}

#[inline(never)]
fn apply_index_task(vals: &mut Vec<Value>, indices: &[Expr], span: Span) -> Result<()> {
    let base_val = vals
        .pop()
        .ok_or_else(|| AlgError::runtime(Some(span), "Pilha de avaliação inconsistente."))?;
    let start = vals
        .len()
        .checked_sub(indices.len())
        .ok_or_else(|| AlgError::runtime(Some(span), "Pilha de avaliação inconsistente."))?;
    let mut idx_vals = Vec::with_capacity(indices.len());
    for (raw_val, idx_expr) in vals.drain(start..).zip(indices.iter()) {
        idx_vals.push(raw_val.as_int(idx_expr.span())?);
    }
    match base_val {
        Value::Array(arr) => vals.push(arr.get("expressão", &idx_vals, span)?.clone()),
        other => return Err(index_type_error("expressão", &other, span)),
    }
    Ok(())
}

pub(crate) fn eval_const_expr(expr: &Expr, consts: &HashMap<String, Value>) -> Result<Value> {
    let mut work = vec![ConstTask::Expr(expr)];
    let mut vals: Vec<Value> = Vec::new();
    let mut steps = 0usize;

    while let Some(task) = work.pop() {
        steps += 1;
        if steps > MAX_EVAL_WORK_STEPS {
            return Err(AlgError::semantic(
                expr.span(),
                "Complexidade máxima de expressão constante excedida.",
            ));
        }

        match task {
            ConstTask::Expr(cur) => match cur {
                Expr::Int(n, _) => vals.push(Value::Inteiro(*n)),
                Expr::Real(f, _) => vals.push(Value::Real(*f)),
                Expr::Str(s, _) => vals.push(Value::Caractere(s.clone())),
                Expr::Bool(b, _) => vals.push(Value::Logico(*b)),
                Expr::Var(name, span) => {
                    let key = name.to_lowercase();
                    if let Some(val) = consts.get(&key) {
                        vals.push(val.clone());
                        continue;
                    }
                    let norm = normalize_keyword(name);
                    if norm == "rand" || norm == "randi" {
                        return Err(AlgError::semantic(
                            *span,
                            format!(
                                "Função não determinística '{name}' não pode ser usada em expressão constante."
                            ),
                        ));
                    }
                    if is_zero_arg_builtin(name) {
                        let mut dummy_rng = RngState::from_seed(1);
                        if let Some(val) = call_builtin(name, Vec::new(), *span, &mut dummy_rng)? {
                            vals.push(val);
                            continue;
                        }
                    }
                    return Err(AlgError::semantic(
                        *span,
                        format!("Constante '{name}' não foi declarada."),
                    ));
                }
                Expr::Call { name, args, span } => {
                    let norm = normalize_keyword(name);
                    if norm == "rand" || norm == "randi" {
                        return Err(AlgError::semantic(
                            *span,
                            format!(
                                "Função não determinística '{name}' não pode ser usada em expressão constante."
                            ),
                        ));
                    }
                    work.push(ConstTask::ApplyCall(name, args.len(), *span));
                    for arg in args.iter().rev() {
                        work.push(ConstTask::Expr(arg));
                    }
                }
                Expr::Unary { op, expr, span } => {
                    work.push(ConstTask::ApplyUnary(*op, *span));
                    work.push(ConstTask::Expr(expr));
                }
                Expr::Binary {
                    left,
                    op,
                    right,
                    span,
                } => {
                    if matches!(op, BinaryOp::And | BinaryOp::Or) {
                        work.push(ConstTask::ShortCircuitRhs(*op, left, right));
                        work.push(ConstTask::Expr(left));
                    } else {
                        work.push(ConstTask::ApplyBinary(*op, *span));
                        work.push(ConstTask::Expr(right));
                        work.push(ConstTask::Expr(left));
                    }
                }
                Expr::Field { span, .. } | Expr::Index { span, .. } => {
                    return Err(AlgError::semantic(
                        *span,
                        "Expressão constante não pode acessar campos ou índices.",
                    ));
                }
            },
            ConstTask::ApplyUnary(op, span) => {
                let v = vals.pop().ok_or_else(|| {
                    AlgError::semantic(span, "Pilha de avaliação constante inconsistente.")
                })?;
                vals.push(eval_unary(op, v, span)?);
            }
            ConstTask::ApplyBinary(op, span) => {
                let (l, r) = pop_two_values(&mut vals, span)?;
                vals.push(eval_binary(l, op, r, span)?);
            }
            ConstTask::ShortCircuitRhs(op, left, right) => {
                let left_span = left.span();
                let lv = vals
                    .pop()
                    .ok_or_else(|| {
                        AlgError::semantic(
                            left_span,
                            "Pilha de avaliação constante inconsistente.",
                        )
                    })?
                    .as_bool(left_span)?;
                match (op, lv) {
                    (BinaryOp::And, false) => vals.push(Value::Logico(false)),
                    (BinaryOp::Or, true) => vals.push(Value::Logico(true)),
                    _ => {
                        work.push(ConstTask::ExpectBool(right));
                        work.push(ConstTask::Expr(right));
                    }
                }
            }
            ConstTask::ExpectBool(e) => {
                let span = e.span();
                let rv = vals
                    .pop()
                    .ok_or_else(|| {
                        AlgError::semantic(
                            span,
                            "Pilha de avaliação constante inconsistente.",
                        )
                    })?
                    .as_bool(span)?;
                vals.push(Value::Logico(rv));
            }
            ConstTask::ApplyCall(name, arg_count, span) => {
                let start = vals.len().checked_sub(arg_count).ok_or_else(|| {
                    AlgError::semantic(span, "Pilha de avaliação constante inconsistente.")
                })?;
                let evaled: Vec<Value> = vals.drain(start..).collect();
                let mut dummy_rng = RngState::from_seed(1);
                let out = call_builtin(name, evaled, span, &mut dummy_rng)?.ok_or_else(|| {
                    AlgError::semantic(
                        span,
                        format!("Função '{name}' não pode ser usada em expressão constante."),
                    )
                })?;
                vals.push(out);
            }
        }
    }

    vals.pop().ok_or_else(|| {
        AlgError::semantic(expr.span(), "Expressão constante sem resultado.")
    })
}

pub(crate) fn eval_unary(op: UnaryOp, val: Value, span: Span) -> Result<Value> {
    match op {
        UnaryOp::Plus => match val {
            Value::Inteiro(_) | Value::Real(_) => Ok(val),
            other => Err(AlgError::runtime(
                Some(span),
                format!("Operador '+' unário inválido para '{}'.", other.type_label()),
            )),
        },
        UnaryOp::Minus => match val {
            Value::Inteiro(n) => n.checked_neg().map(Value::Inteiro).ok_or_else(|| {
                AlgError::runtime(Some(span), "Estouro de inteiro na negação.")
            }),
            Value::Real(f) => Ok(Value::Real(-f)),
            other => Err(AlgError::runtime(
                Some(span),
                format!("Operador '-' unário inválido para '{}'.", other.type_label()),
            )),
        },
        UnaryOp::Not => Ok(Value::Logico(!val.as_bool(span)?)),
    }
}

fn eval_num_arith(
    left: Value,
    right: Value,
    span: Span,
    op_label: &str,
    int_op: impl FnOnce(i64, i64) -> Option<i64>,
    int_overflow_msg: &str,
    real_op: impl FnOnce(f64, f64) -> f64,
) -> Result<Value> {
    match (left, right) {
        (Value::Inteiro(a), Value::Inteiro(b)) => int_op(a, b)
            .map(Value::Inteiro)
            .ok_or_else(|| AlgError::runtime(Some(span), int_overflow_msg)),
        (Value::Inteiro(a), Value::Real(b)) => {
            Ok(Value::Real(ensure_finite(real_op(a as f64, b), op_label, span)?))
        }
        (Value::Real(a), Value::Inteiro(b)) => {
            Ok(Value::Real(ensure_finite(real_op(a, b as f64), op_label, span)?))
        }
        (Value::Real(a), Value::Real(b)) => {
            Ok(Value::Real(ensure_finite(real_op(a, b), op_label, span)?))
        }
        (l, r) => Err(AlgError::runtime(
            Some(span),
            format!(
                "Operação '{op_label}' inválida entre '{}' e '{}'.",
                l.type_label(),
                r.type_label()
            ),
        )),
    }
}

pub(crate) fn eval_binary(left: Value, op: BinaryOp, right: Value, span: Span) -> Result<Value> {
    match op {
        BinaryOp::Add => match (left, right) {
            (Value::Caractere(a), Value::Caractere(b)) => {
                let total = a.len().saturating_add(b.len());
                ensure_string_len(total, Some(span))?;
                let mut out = String::with_capacity(total);
                out.push_str(&a);
                out.push_str(&b);
                Ok(Value::Caractere(out))
            }
            (l, r) => eval_num_arith(
                l,
                r,
                span,
                "+",
                i64::checked_add,
                "Estouro de inteiro na soma.",
                |a, b| a + b,
            ),
        },
        BinaryOp::Sub => eval_num_arith(
            left,
            right,
            span,
            "-",
            i64::checked_sub,
            "Estouro de inteiro na subtração.",
            |a, b| a - b,
        ),
        BinaryOp::Mul => eval_num_arith(
            left,
            right,
            span,
            "*",
            i64::checked_mul,
            "Estouro de inteiro na multiplicação.",
            |a, b| a * b,
        ),
        BinaryOp::Div => {
            let a = left.as_number(span)?;
            let b = right.as_number(span)?;
            if b == 0.0 {
                return Err(AlgError::runtime(Some(span), "Divisão por zero."));
            }
            Ok(Value::Real(ensure_finite(a / b, "/", span)?))
        }
        BinaryOp::IntDiv => {
            let a = left.as_int(span)?;
            let b = right.as_int(span)?;
            if b == 0 {
                return Err(AlgError::runtime(
                    Some(span),
                    "Divisão inteira por zero.",
                ));
            }
            a.checked_div(b).map(Value::Inteiro).ok_or_else(|| {
                AlgError::runtime(Some(span), "Estouro de inteiro na divisão inteira.")
            })
        }
        BinaryOp::Mod => {
            let a = left.as_int(span)?;
            let b = right.as_int(span)?;
            if b == 0 {
                return Err(AlgError::runtime(
                    Some(span),
                    "Resto de divisão ('mod') por zero.",
                ));
            }
            a.checked_rem(b).map(Value::Inteiro).ok_or_else(|| {
                AlgError::runtime(Some(span), "Estouro de inteiro no resto de divisão ('mod').")
            })
        }
        BinaryOp::Pow => {
            let a = left.as_number(span)?;
            let b = right.as_number(span)?;
            Ok(Value::Real(eval_pow(a, b, "^", span)?))
        }
        BinaryOp::Eq => Ok(Value::Logico(values_equal(&left, &right, span)?)),
        BinaryOp::Neq => Ok(Value::Logico(!values_equal(&left, &right, span)?)),
        BinaryOp::Lt => Ok(Value::Logico(
            compare_values(&left, &right, span)? == std::cmp::Ordering::Less,
        )),
        BinaryOp::Le => Ok(Value::Logico(
            compare_values(&left, &right, span)? != std::cmp::Ordering::Greater,
        )),
        BinaryOp::Gt => Ok(Value::Logico(
            compare_values(&left, &right, span)? == std::cmp::Ordering::Greater,
        )),
        BinaryOp::Ge => Ok(Value::Logico(
            compare_values(&left, &right, span)? != std::cmp::Ordering::Less,
        )),
        BinaryOp::Xor => {
            let a = left.as_bool(span)?;
            let b = right.as_bool(span)?;
            Ok(Value::Logico(a ^ b))
        }
        BinaryOp::And | BinaryOp::Or => unreachable!("curto-circuito tratado acima"),
    }
}

pub(crate) fn values_equal(left: &Value, right: &Value, span: Span) -> Result<bool> {
    match (left, right) {
        (Value::Inteiro(a), Value::Inteiro(b)) => Ok(a == b),
        (Value::Inteiro(a), Value::Real(b)) => Ok((*a as f64) == *b),
        (Value::Real(a), Value::Inteiro(b)) => Ok(*a == (*b as f64)),
        (Value::Real(a), Value::Real(b)) => Ok(a == b),
        (Value::Caractere(a), Value::Caractere(b)) => Ok(a == b),
        (Value::Logico(a), Value::Logico(b)) => Ok(a == b),
        (l, r) => Err(AlgError::runtime(
            Some(span),
            format!(
                "Não é possível comparar valores de tipos '{}' e '{}'.",
                l.type_label(),
                r.type_label()
            ),
        )),
    }
}

pub(crate) fn compare_values(
    left: &Value,
    right: &Value,
    span: Span,
) -> Result<std::cmp::Ordering> {
    match (left, right) {
        (Value::Inteiro(a), Value::Inteiro(b)) => Ok(a.cmp(b)),
        (Value::Inteiro(a), Value::Real(b)) => (*a as f64).partial_cmp(b).ok_or_else(|| {
            AlgError::runtime(Some(span), "Comparação numérica inválida (NaN).")
        }),
        (Value::Real(a), Value::Inteiro(b)) => a.partial_cmp(&(*b as f64)).ok_or_else(|| {
            AlgError::runtime(Some(span), "Comparação numérica inválida (NaN).")
        }),
        (Value::Real(a), Value::Real(b)) => a.partial_cmp(b).ok_or_else(|| {
            AlgError::runtime(Some(span), "Comparação numérica inválida (NaN).")
        }),
        (Value::Caractere(a), Value::Caractere(b)) => Ok(a.cmp(b)),
        (l, r) => Err(AlgError::runtime(
            Some(span),
            format!(
                "Não é possível ordenar valores de tipos '{}' e '{}'.",
                l.type_label(),
                r.type_label()
            ),
        )),
    }
}
