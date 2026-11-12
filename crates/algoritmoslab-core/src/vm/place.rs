use super::eval::eval_const_expr;
use crate::ast::{AccessStep, Expr, LValue, TypeDecl, VarType};
use crate::error::{Result, Span, AlgError};
use crate::value::{RecordFieldSpec, RecordTypeSpec, ResolvedArrayRange, ResolvedType, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct VarSlot {
    pub root_name: String,
    pub resolved_type: ResolvedType,
    pub is_const: bool,
    pub value: Value,
}

pub(crate) type SlotRef = Rc<RefCell<VarSlot>>;

#[derive(Debug, Clone)]
pub(crate) enum SlotBinding {
    Slot {
        display_name: String,
        slot: SlotRef,
    },
    SubPlace {
        display_name: String,
        declared_type: ResolvedType,
        slot: SlotRef,
        steps: Vec<ResolvedStep>,
    },
}

impl SlotBinding {
    pub(crate) fn display_name(&self) -> &str {
        match self {
            SlotBinding::Slot { display_name, .. } | SlotBinding::SubPlace { display_name, .. } => {
                display_name
            }
        }
    }

    pub(crate) fn slot_ref(&self) -> &SlotRef {
        match self {
            SlotBinding::Slot { slot, .. } | SlotBinding::SubPlace { slot, .. } => slot,
        }
    }

    pub(crate) fn is_const(&self) -> bool {
        self.slot_ref().borrow().is_const
    }

    pub(crate) fn read_value(&self) -> Result<Value> {
        match self {
            SlotBinding::Slot { slot, .. } => Ok(slot.borrow().value.clone()),
            SlotBinding::SubPlace {
                display_name,
                slot,
                steps,
                ..
            } => {
                let borrowed = slot.borrow();
                Ok(read_at_path(&borrowed.value, display_name, steps)?.clone())
            }
        }
    }

    pub(crate) fn write_value(&self, new_val: Value, assign_span: Span) -> Result<()> {
        match self {
            SlotBinding::Slot { display_name, slot } => {
                let mut borrowed = slot.borrow_mut();
                if borrowed.is_const {
                    return Err(AlgError::runtime(
                        Some(assign_span),
                        format!("Não é possível alterar o valor da constante '{display_name}'."),
                    ));
                }
                let expected = borrowed.resolved_type.clone();
                borrowed.value = new_val.coerce_to_resolved(&expected, Some(assign_span))?;
                Ok(())
            }
            SlotBinding::SubPlace {
                display_name,
                slot,
                steps,
                ..
            } => {
                let mut borrowed = slot.borrow_mut();
                if borrowed.is_const {
                    return Err(AlgError::runtime(
                        Some(assign_span),
                        format!("Não é possível alterar o valor da constante '{display_name}'."),
                    ));
                }
                write_at_path_mut(&mut borrowed.value, display_name, steps, new_val, assign_span)
            }
        }
    }

    pub(crate) fn resolved_type(&self) -> ResolvedType {
        match self {
            SlotBinding::Slot { slot, .. } => slot.borrow().resolved_type.clone(),
            SlotBinding::SubPlace { declared_type, .. } => declared_type.clone(),
        }
    }
}

pub(crate) struct ScopeFrame {
    pub label: String,
    pub order: Vec<String>,
    pub bindings: HashMap<String, SlotBinding>,
    pub allocated_cells: usize,
}

impl ScopeFrame {
    pub(crate) fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            order: Vec::new(),
            bindings: HashMap::new(),
            allocated_cells: 0,
        }
    }

    pub(crate) fn insert(&mut self, key: String, binding: SlotBinding) {
        if !self.bindings.contains_key(&key) {
            self.order.push(key.clone());
        }
        self.bindings.insert(key, binding);
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ResolvedStep {
    Field(String, Span),
    Index(Vec<i64>, Span),
}

pub(crate) struct ResolvedPlace {
    pub slot: SlotRef,
    pub base_name: String,
    pub display_label: String,
    pub steps: Vec<ResolvedStep>,
    pub span: Span,
}

pub(crate) fn expr_to_lvalue(expr: &Expr) -> Option<LValue> {
    if !matches!(
        expr,
        Expr::Var(_, _) | Expr::Field { .. } | Expr::Index { .. }
    ) {
        return None;
    }

    let mut steps = Vec::new();
    let mut cur = expr;
    let (base, base_span) = loop {
        match cur {
            Expr::Var(name, span) => break (name.clone(), *span),
            Expr::Field {
                target,
                field,
                span,
            } => {
                steps.push(AccessStep::Field {
                    name: field.clone(),
                    span: *span,
                });
                cur = target;
            }
            Expr::Index {
                target,
                indices,
                span,
            } => {
                steps.push(AccessStep::Index {
                    indices: indices.clone(),
                    span: *span,
                });
                cur = target;
            }
            _ => return None,
        }
    };
    steps.reverse();
    Some(LValue {
        base,
        base_span,
        steps,
        span: expr.span(),
    })
}

pub(crate) fn format_place_label(base: &str, steps: &[ResolvedStep]) -> String {
    let mut out = base.to_string();
    for step in steps {
        append_step_desc(&mut out, step);
    }
    out
}

fn append_step_desc(path_desc: &mut String, step: &ResolvedStep) {
    match step {
        ResolvedStep::Field(name, _) => {
            path_desc.push('.');
            path_desc.push_str(name);
        }
        ResolvedStep::Index(indices, _) => {
            let idx_s = indices
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            path_desc.push('[');
            path_desc.push_str(&idx_s);
            path_desc.push(']');
        }
    }
}

pub(crate) fn field_not_record_error(
    target_desc: &str,
    field: &str,
    other: &Value,
    span: Span,
) -> AlgError {
    AlgError::runtime(
        Some(span),
        format!(
            "'{target_desc}' não é um registro (é '{}'), portanto não possui campo '.{field}'.",
            other.type_label()
        ),
    )
}

pub(crate) fn index_type_error(target_desc: &str, other: &Value, span: Span) -> AlgError {
    if matches!(other, Value::Caractere(_)) {
        AlgError::runtime(
            Some(span),
            format!(
                "'{target_desc}' é do tipo 'caractere' e não suporta indexação '[...]' (dica: use copia(s, i, 1))."
            ),
        )
    } else {
        AlgError::runtime(
            Some(span),
            format!(
                "'{target_desc}' não é um vetor ou matriz (é '{}').",
                other.type_label()
            ),
        )
    }
}

macro_rules! walk_resolved_steps {
    ($cur:expr, $base_name:expr, $steps:expr, $get_field:ident, $get_arr:ident) => {{
        let mut path_desc = $base_name.to_string();
        for step in $steps {
            match step {
                ResolvedStep::Field(field, fspan) => match $cur {
                    Value::Record(rec) => {
                        $cur = rec.$get_field(field, *fspan)?;
                        append_step_desc(&mut path_desc, step);
                    }
                    other => {
                        return Err(field_not_record_error(&path_desc, field, other, *fspan));
                    }
                },
                ResolvedStep::Index(indices, ispan) => match $cur {
                    Value::Array(arr) => {
                        $cur = arr.$get_arr(&path_desc, indices, *ispan)?;
                        append_step_desc(&mut path_desc, step);
                    }
                    other => {
                        return Err(index_type_error(&path_desc, other, *ispan));
                    }
                },
            }
        }
        Ok($cur)
    }};
}

pub(crate) fn read_at_path<'v>(
    mut cur: &'v Value,
    base_name: &str,
    steps: &[ResolvedStep],
) -> Result<&'v Value> {
    walk_resolved_steps!(cur, base_name, steps, get_field, get)
}

pub(crate) fn write_resolved_place(
    place: &ResolvedPlace,
    new_val: Value,
    assign_span: Span,
) -> Result<()> {
    let mut borrowed = place.slot.borrow_mut();
    if borrowed.is_const {
        return Err(AlgError::runtime(
            Some(assign_span),
            format!(
                "Não é possível alterar o valor da constante '{}'.",
                borrowed.root_name
            ),
        ));
    }
    write_at_path_mut(
        &mut borrowed.value,
        &place.base_name,
        &place.steps,
        new_val,
        assign_span,
    )
}

pub(crate) fn write_at_path_mut(
    mut cur: &mut Value,
    base_name: &str,
    steps: &[ResolvedStep],
    new_val: Value,
    assign_span: Span,
) -> Result<()> {
    let leaf = walk_resolved_steps!(cur, base_name, steps, get_field_mut, get_mut)?;
    let expected = leaf.resolved_type();
    *leaf = new_val.coerce_to_resolved(&expected, Some(assign_span))?;
    Ok(())
}

pub(crate) const MAX_TYPE_RESOLUTION_DEPTH: usize = 256;

pub(crate) fn resolve_named_type(
    key: &str,
    ref_span: Span,
    raw_types: &HashMap<String, &TypeDecl>,
    resolved_types: &mut HashMap<String, ResolvedType>,
    consts: &HashMap<String, Value>,
    visiting: &mut HashSet<String>,
) -> Result<ResolvedType> {
    if let Some(existing) = resolved_types.get(key) {
        return Ok(existing.clone());
    }
    let decl = raw_types.get(key).copied().ok_or_else(|| {
        AlgError::semantic(ref_span, format!("Tipo '{key}' não foi declarado."))
    })?;
    if !visiting.insert(key.to_string()) {
        return Err(AlgError::semantic(
            decl.span,
            format!("Referência circular na definição do tipo '{}'.", decl.name),
        ));
    }
    if visiting.len() > MAX_TYPE_RESOLUTION_DEPTH {
        visiting.remove(key);
        return Err(AlgError::semantic(
            decl.span,
            format!(
                "Profundidade máxima de resolução de tipos ({MAX_TYPE_RESOLUTION_DEPTH}) excedida em '{}'.",
                decl.name
            ),
        ));
    }
    let resolved = resolve_var_type_spec(
        &decl.def,
        raw_types,
        resolved_types,
        consts,
        visiting,
        Some(&decl.name),
    );
    visiting.remove(key);
    let ty = resolved?;
    resolved_types.insert(key.to_string(), ty.clone());
    Ok(ty)
}

pub(crate) fn resolve_var_type_spec(
    vt: &VarType,
    raw_types: &HashMap<String, &TypeDecl>,
    resolved_types: &mut HashMap<String, ResolvedType>,
    consts: &HashMap<String, Value>,
    visiting: &mut HashSet<String>,
    type_name_hint: Option<&str>,
) -> Result<ResolvedType> {
    match vt {
        VarType::Primitive(p) => Ok(ResolvedType::Primitive(*p)),
        VarType::Named(name, span) => {
            let key = name.to_lowercase();
            if let Some(existing) = resolved_types.get(&key) {
                return Ok(existing.clone());
            }
            if raw_types.contains_key(&key) {
                return resolve_named_type(
                    &key,
                    *span,
                    raw_types,
                    resolved_types,
                    consts,
                    visiting,
                );
            }
            Err(AlgError::semantic(
                *span,
                format!("Tipo '{name}' não foi declarado."),
            ))
        }
        VarType::Record { fields, .. } => {
            let mut seen = HashSet::new();
            let mut resolved_fields = Vec::with_capacity(fields.len());
            for f in fields {
                let fkey = f.name.to_lowercase();
                if !seen.insert(fkey.clone()) {
                    return Err(AlgError::semantic(
                        f.span,
                        format!("Campo '{}' declarado em duplicidade no registro.", f.name),
                    ));
                }
                let fty = resolve_var_type_spec(
                    &f.var_type,
                    raw_types,
                    resolved_types,
                    consts,
                    visiting,
                    None,
                )?;
                resolved_fields.push(RecordFieldSpec {
                    key: fkey,
                    name: f.name.clone(),
                    ty: fty,
                });
            }
            Ok(ResolvedType::Record(Arc::new(RecordTypeSpec {
                type_name: type_name_hint.map(ToString::to_string),
                fields: resolved_fields,
            })))
        }
        VarType::Array { elem_type, dims } => {
            let mut resolved_dims = Vec::with_capacity(dims.len());
            for d in dims {
                let start = eval_const_expr(&d.start, consts)?.as_int(d.start.span())?;
                let end = eval_const_expr(&d.end, consts)?.as_int(d.end.span())?;
                resolved_dims.push(ResolvedArrayRange { start, end });
            }
            let inner = resolve_var_type_spec(
                elem_type,
                raw_types,
                resolved_types,
                consts,
                visiting,
                None,
            )?;
            Ok(ResolvedType::Array {
                elem_type: Box::new(inner),
                dims: resolved_dims,
            })
        }
    }
}
