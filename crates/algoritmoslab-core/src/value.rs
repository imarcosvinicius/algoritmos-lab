use crate::ast::PrimitiveType;
use crate::error::{Result, Span, AlgError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const MAX_ARRAY_ELEMENTS: usize = 1_000_000;
pub const MAX_VM_TOTAL_ELEMENTS: usize = 4_000_000;
pub const MAX_FORMAT_WIDTH: usize = 4096;
pub const MAX_FORMAT_DECIMALS: usize = 20;
pub const MAX_STRING_BYTES: usize = 16 * 1024 * 1024;
const MAX_PREVIEW_ITEM_CHARS: usize = 120;
const MAX_PREVIEW_DEPTH: usize = 4;

fn truncate_preview_text(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let prefix: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{prefix}...")
    } else {
        prefix
    }
}

fn preview_display_item(val: &Value, depth: usize) -> String {
    match val {
        Value::Caractere(s) => truncate_preview_text(s, MAX_PREVIEW_ITEM_CHARS),
        Value::Record(rec) => {
            if depth >= MAX_PREVIEW_DEPTH {
                format!("{{{} ...}}", rec.type_label())
            } else {
                rec.preview_with_depth(depth + 1)
            }
        }
        Value::Array(arr) => {
            if depth >= MAX_PREVIEW_DEPTH {
                format!("[... ({} itens)]", arr.elements.len())
            } else {
                arr.preview_with_depth(depth + 1)
            }
        }
        _ => val.to_display_string(),
    }
}

/// Valida se o comprimento em bytes de um texto não excede [`MAX_STRING_BYTES`].
pub fn ensure_string_len(len: usize, span: Option<Span>) -> Result<()> {
    if len > MAX_STRING_BYTES {
        return Err(AlgError::runtime(
            span,
            format!("Tamanho máximo de texto ({MAX_STRING_BYTES} bytes) excedido."),
        ));
    }
    Ok(())
}

/// Calcula o comprimento de um intervalo `[start..end]` de forma segura contra overflow e truncamento.
pub fn checked_range_len(start: i64, end: i64) -> Result<usize> {
    if end < start {
        return Err(AlgError::runtime(
            None,
            format!("Intervalo inválido [{start}..{end}]: o início deve ser menor ou igual ao fim."),
        ));
    }
    let len_i128 = end as i128 - start as i128 + 1;
    usize::try_from(len_i128).map_err(|_| {
        AlgError::runtime(
            None,
            format!("Intervalo [{start}..{end}] excede a capacidade máxima endereçável."),
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedArrayRange {
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordFieldSpec {
    pub key: String,
    pub name: String,
    pub ty: ResolvedType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordTypeSpec {
    pub type_name: Option<String>,
    pub fields: Vec<RecordFieldSpec>,
}

impl RecordTypeSpec {
    pub fn type_label(&self) -> String {
        self.type_name
            .clone()
            .unwrap_or_else(|| "registro".to_string())
    }
}

pub(crate) fn format_array_type_label(
    dims: &[ResolvedArrayRange],
    elem_type: &ResolvedType,
) -> String {
    let ranges: Vec<String> = dims
        .iter()
        .map(|d| format!("{}..{}", d.start, d.end))
        .collect();
    format!("vetor[{}] de {}", ranges.join(", "), elem_type.type_label())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedType {
    Primitive(PrimitiveType),
    Record(Arc<RecordTypeSpec>),
    Array {
        elem_type: Box<ResolvedType>,
        dims: Vec<ResolvedArrayRange>,
    },
}

impl ResolvedType {
    pub fn type_label(&self) -> String {
        match self {
            ResolvedType::Primitive(p) => p.label().to_string(),
            ResolvedType::Record(spec) => spec.type_label(),
            ResolvedType::Array { elem_type, dims } => format_array_type_label(dims, elem_type),
        }
    }

    pub fn matches(&self, other: &ResolvedType) -> bool {
        match (self, other) {
            (ResolvedType::Primitive(a), ResolvedType::Primitive(b)) => a == b,
            (
                ResolvedType::Array {
                    elem_type: e1,
                    dims: d1,
                },
                ResolvedType::Array {
                    elem_type: e2,
                    dims: d2,
                },
            ) => d1 == d2 && e1.matches(e2),
            (ResolvedType::Record(r1), ResolvedType::Record(r2)) => {
                if let (Some(n1), Some(n2)) = (&r1.type_name, &r2.type_name) {
                    return n1.to_lowercase() == n2.to_lowercase();
                }
                if r1.fields.len() != r2.fields.len() {
                    return false;
                }
                r1.fields
                    .iter()
                    .zip(r2.fields.iter())
                    .all(|(f1, f2)| f1.key == f2.key && f1.ty.matches(&f2.ty))
            }
            _ => false,
        }
    }

    pub fn checked_leaf_count(&self) -> Result<usize> {
        match self {
            ResolvedType::Primitive(_) => Ok(1),
            ResolvedType::Record(spec) => {
                if spec.fields.is_empty() {
                    return Ok(1);
                }
                let mut total = 0usize;
                for f in &spec.fields {
                    let sub = f.ty.checked_leaf_count()?;
                    total = total.checked_add(sub).ok_or_else(|| {
                        AlgError::runtime(
                            None,
                            "Estrutura excede a capacidade máxima de elementos.",
                        )
                    })?;
                }
                Ok(total.max(1))
            }
            ResolvedType::Array { elem_type, dims } => {
                let mut total = elem_type.checked_leaf_count()?;
                for d in dims {
                    let len = checked_range_len(d.start, d.end)?;
                    total = total.checked_mul(len).ok_or_else(|| {
                        AlgError::runtime(
                            None,
                            "Dimensões do vetor/matriz excedem a capacidade máxima endereçável.",
                        )
                    })?;
                }
                Ok(total)
            }
        }
    }

    #[cfg(test)]
    pub fn leaf_count(&self) -> usize {
        self.checked_leaf_count().unwrap_or(usize::MAX)
    }

    fn snapshot_leaf_count(&self) -> usize {
        match self {
            ResolvedType::Primitive(_) => 1,
            ResolvedType::Record(spec) => spec
                .fields
                .iter()
                .fold(0usize, |acc, f| acc.saturating_add(f.ty.snapshot_leaf_count())),
            ResolvedType::Array { elem_type, dims } => {
                let elem_leaves = elem_type.snapshot_leaf_count();
                dims.iter().fold(elem_leaves, |acc, d| {
                    let len = checked_range_len(d.start, d.end).unwrap_or(usize::MAX);
                    acc.saturating_mul(len)
                })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordValue {
    pub spec: Arc<RecordTypeSpec>,
    pub fields: Vec<Value>,
}

impl RecordValue {
    pub fn new(name: &str, spec: Arc<RecordTypeSpec>, span: Span) -> Result<Self> {
        let mut values = Vec::with_capacity(spec.fields.len());
        for field in &spec.fields {
            let field_label = format!("{name}.{}", field.name);
            values.push(Value::allocate_resolved(&field_label, &field.ty, span)?);
        }
        Ok(Self {
            spec,
            fields: values,
        })
    }

    pub fn find_field_idx(&self, field_name: &str) -> Option<usize> {
        let key = field_name.to_lowercase();
        self.spec.fields.iter().position(|f| f.key == key)
    }

    pub fn get_field(&self, field_name: &str, span: Span) -> Result<&Value> {
        let idx = self.find_field_idx(field_name).ok_or_else(|| {
            AlgError::runtime(
                Some(span),
                format!(
                    "Campo '{field_name}' não existe no registro '{}'.",
                    self.type_label()
                ),
            )
        })?;
        Ok(&self.fields[idx])
    }

    pub fn get_field_mut(&mut self, field_name: &str, span: Span) -> Result<&mut Value> {
        let idx = self.find_field_idx(field_name).ok_or_else(|| {
            AlgError::runtime(
                Some(span),
                format!(
                    "Campo '{field_name}' não existe no registro '{}'.",
                    self.type_label()
                ),
            )
        })?;
        Ok(&mut self.fields[idx])
    }

    pub fn set_field(&mut self, field_name: &str, value: Value, span: Span) -> Result<()> {
        let idx = self.find_field_idx(field_name).ok_or_else(|| {
            AlgError::runtime(
                Some(span),
                format!(
                    "Campo '{field_name}' não existe no registro '{}'.",
                    self.type_label()
                ),
            )
        })?;
        let target_ty = self.spec.fields[idx].ty.clone();
        self.fields[idx] = value.coerce_to_resolved(&target_ty, Some(span))?;
        Ok(())
    }

    pub fn type_label(&self) -> String {
        self.spec.type_label()
    }

    pub fn preview(&self) -> String {
        self.preview_with_depth(0)
    }

    fn preview_with_depth(&self, depth: usize) -> String {
        let max_show = 4usize;
        let parts: Vec<String> = self
            .spec
            .fields
            .iter()
            .zip(self.fields.iter())
            .take(max_show)
            .map(|(spec, val)| format!("{}: {}", spec.name, preview_display_item(val, depth)))
            .collect();
        if self.spec.fields.len() > max_show {
            format!(
                "{{{}, ... ({} campos)}}",
                parts.join(", "),
                self.spec.fields.len()
            )
        } else {
            format!("{{{}}}", parts.join(", "))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArrayValue {
    pub elem_type: ResolvedType,
    pub dims: Vec<ResolvedArrayRange>,
    pub elements: Vec<Value>,
}

impl ArrayValue {
    pub fn new(
        name: &str,
        elem_type: ResolvedType,
        dims: Vec<ResolvedArrayRange>,
        span: Span,
    ) -> Result<Self> {
        let exceed_err = || {
            AlgError::runtime(
                Some(span),
                format!(
                    "Vetor/matriz '{}' excede o limite de segurança ({} elementos).",
                    name, MAX_ARRAY_ELEMENTS
                ),
            )
        };
        let elem_leaves = elem_type.checked_leaf_count().map_err(|_| exceed_err())?;
        let mut total: usize = 1;
        for d in &dims {
            if d.end < d.start {
                return Err(AlgError::semantic(
                    span,
                    format!(
                        "Intervalo inválido [{}..{}] no vetor '{}': o início deve ser menor ou igual ao fim.",
                        d.start, d.end, name
                    ),
                ));
            }
            let len = checked_range_len(d.start, d.end).map_err(|_| exceed_err())?;
            total = total.checked_mul(len).ok_or_else(exceed_err)?;
            let approx_slots = total.checked_mul(elem_leaves).ok_or_else(exceed_err)?;
            if total > MAX_ARRAY_ELEMENTS || approx_slots > MAX_ARRAY_ELEMENTS {
                return Err(exceed_err());
            }
        }
        let default_val = Value::allocate_resolved(name, &elem_type, span)?;
        Ok(Self {
            elem_type,
            dims,
            elements: vec![default_val; total],
        })
    }

    pub fn flat_index(&self, name: &str, indices: &[i64], span: Span) -> Result<usize> {
        if indices.len() != self.dims.len() {
            return Err(AlgError::runtime(
                Some(span),
                format!(
                    "'{}' esperava {} índice(s), mas recebeu {}.",
                    name,
                    self.dims.len(),
                    indices.len()
                ),
            ));
        }
        let mut flat = 0usize;
        let mut stride = 1usize;
        for (pos, (idx, dim)) in indices.iter().rev().zip(self.dims.iter().rev()).enumerate() {
            if *idx < dim.start || *idx > dim.end {
                return Err(AlgError::runtime(
                    Some(span),
                    format!(
                        "Índice {} fora dos limites [{}..{}] de '{}'.",
                        idx, dim.start, dim.end, name
                    ),
                ));
            }
            let offset = usize::try_from(*idx as i128 - dim.start as i128).map_err(|_| {
                AlgError::runtime(Some(span), format!("Índice inválido em '{name}'."))
            })?;
            let term = offset.checked_mul(stride).ok_or_else(|| {
                AlgError::runtime(Some(span), format!("Estouro de índice em '{name}'."))
            })?;
            flat = flat.checked_add(term).ok_or_else(|| {
                AlgError::runtime(Some(span), format!("Estouro de índice em '{name}'."))
            })?;

            if pos + 1 < self.dims.len() {
                let dim_len = checked_range_len(dim.start, dim.end).map_err(|_| {
                    AlgError::runtime(Some(span), format!("Dimensão inválida em '{name}'."))
                })?;
                stride = stride.checked_mul(dim_len).ok_or_else(|| {
                    AlgError::runtime(Some(span), format!("Estouro de índice em '{name}'."))
                })?;
            }
        }
        Ok(flat)
    }

    pub fn get(&self, name: &str, indices: &[i64], span: Span) -> Result<&Value> {
        let idx = self.flat_index(name, indices, span)?;
        Ok(&self.elements[idx])
    }

    pub fn get_mut(&mut self, name: &str, indices: &[i64], span: Span) -> Result<&mut Value> {
        let idx = self.flat_index(name, indices, span)?;
        Ok(&mut self.elements[idx])
    }

    pub fn set(&mut self, name: &str, indices: &[i64], value: Value, span: Span) -> Result<()> {
        let coerced = value.coerce_to_resolved(&self.elem_type, Some(span))?;
        let idx = self.flat_index(name, indices, span)?;
        self.elements[idx] = coerced;
        Ok(())
    }

    pub fn type_label(&self) -> String {
        format_array_type_label(&self.dims, &self.elem_type)
    }

    pub fn preview(&self) -> String {
        self.preview_with_depth(0)
    }

    fn preview_with_depth(&self, depth: usize) -> String {
        let max_show = 12usize;
        let parts: Vec<String> = self
            .elements
            .iter()
            .take(max_show)
            .map(|v| preview_display_item(v, depth))
            .collect();
        if self.elements.len() > max_show {
            format!("[{}, ... ({} itens)]", parts.join(", "), self.elements.len())
        } else {
            format!("[{}]", parts.join(", "))
        }
    }

    pub fn coord_label(&self, flat: usize) -> String {
        let mut rem = flat;
        let mut coords = vec![0i128; self.dims.len()];
        for (pos, dim) in self.dims.iter().enumerate().rev() {
            let len = checked_range_len(dim.start, dim.end).unwrap_or(1).max(1);
            let offset = rem % len;
            rem /= len;
            coords[pos] = dim.start as i128 + offset as i128;
        }
        let idx_text = coords
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("[{idx_text}]")
    }

    pub fn snapshot_items(&self, limit: usize) -> Vec<VarArrayItem> {
        let mut items = Vec::with_capacity(limit.min(self.elements.len()));
        collect_array_snapshot_items(self, "", &mut items, limit);
        items
    }
}

fn collect_array_snapshot_items(
    arr: &ArrayValue,
    prefix: &str,
    out: &mut Vec<VarArrayItem>,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }
    for (flat, val) in arr.elements.iter().enumerate() {
        if out.len() >= limit {
            break;
        }
        let idx_label = format!("{prefix}{}", arr.coord_label(flat));
        collect_value_snapshot_items(val, &idx_label, out, limit);
    }
}

fn collect_value_snapshot_items(
    val: &Value,
    prefix: &str,
    out: &mut Vec<VarArrayItem>,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }
    match val {
        Value::Array(nested) => {
            collect_array_snapshot_items(nested, prefix, out, limit);
        }
        Value::Record(rec) => {
            for (spec, fval) in rec.spec.fields.iter().zip(rec.fields.iter()) {
                if out.len() >= limit {
                    break;
                }
                let path_label = format!("{prefix}.{}", spec.name);
                collect_value_snapshot_items(fval, &path_label, out, limit);
            }
        }
        _ => {
            out.push(VarArrayItem {
                index_label: prefix.to_string(),
                value_text: val.to_display_string(),
            });
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Inteiro(i64),
    Real(f64),
    Caractere(String),
    Logico(bool),
    Record(Box<RecordValue>),
    Array(Box<ArrayValue>),
}

impl Value {
    pub fn default_for(ty: PrimitiveType) -> Self {
        match ty {
            PrimitiveType::Inteiro => Value::Inteiro(0),
            PrimitiveType::Real => Value::Real(0.0),
            PrimitiveType::Caractere => Value::Caractere(String::new()),
            PrimitiveType::Logico => Value::Logico(false),
        }
    }

    pub fn allocate_resolved(name: &str, ty: &ResolvedType, span: Span) -> Result<Self> {
        match ty {
            ResolvedType::Primitive(p) => Ok(Self::default_for(*p)),
            ResolvedType::Record(spec) => Ok(Value::Record(Box::new(RecordValue::new(
                name,
                Arc::clone(spec),
                span,
            )?))),
            ResolvedType::Array { elem_type, dims } => Ok(Value::Array(Box::new(ArrayValue::new(
                name,
                elem_type.as_ref().clone(),
                dims.clone(),
                span,
            )?))),
        }
    }

    pub fn resolved_type(&self) -> ResolvedType {
        match self {
            Value::Inteiro(_) => ResolvedType::Primitive(PrimitiveType::Inteiro),
            Value::Real(_) => ResolvedType::Primitive(PrimitiveType::Real),
            Value::Caractere(_) => ResolvedType::Primitive(PrimitiveType::Caractere),
            Value::Logico(_) => ResolvedType::Primitive(PrimitiveType::Logico),
            Value::Record(rec) => ResolvedType::Record(Arc::clone(&rec.spec)),
            Value::Array(arr) => ResolvedType::Array {
                elem_type: Box::new(arr.elem_type.clone()),
                dims: arr.dims.clone(),
            },
        }
    }

    pub fn type_label(&self) -> String {
        match self {
            Value::Inteiro(_) => "inteiro".to_string(),
            Value::Real(_) => "real".to_string(),
            Value::Caractere(_) => "caractere".to_string(),
            Value::Logico(_) => "logico".to_string(),
            Value::Record(rec) => rec.type_label(),
            Value::Array(arr) => arr.type_label(),
        }
    }

    pub fn coerce_to(self, target: PrimitiveType, span: Option<Span>) -> Result<Self> {
        match (self, target) {
            (Value::Inteiro(n), PrimitiveType::Inteiro) => Ok(Value::Inteiro(n)),
            (Value::Inteiro(n), PrimitiveType::Real) => Ok(Value::Real(n as f64)),
            (Value::Real(f), PrimitiveType::Real) => Ok(Value::Real(f)),
            (Value::Real(_), PrimitiveType::Inteiro) => Err(AlgError::runtime(
                span,
                "Não é possível atribuir um valor 'real' a uma variável 'inteiro' (dica: use 'int(...)' ou '\\' para divisão inteira).",
            )),
            (Value::Caractere(s), PrimitiveType::Caractere) => Ok(Value::Caractere(s)),
            (Value::Logico(b), PrimitiveType::Logico) => Ok(Value::Logico(b)),
            (other, t) => Err(AlgError::runtime(
                span,
                format!(
                    "Incompatibilidade de tipos: esperado '{}', mas recebeu '{}'.",
                    t.label(),
                    other.type_label()
                ),
            )),
        }
    }

    pub fn coerce_to_resolved(self, target: &ResolvedType, span: Option<Span>) -> Result<Self> {
        match target {
            ResolvedType::Primitive(p) => self.coerce_to(*p, span),
            ResolvedType::Record(spec) => match self {
                Value::Record(rec)
                    if ResolvedType::Record(Arc::clone(&rec.spec)).matches(target) =>
                {
                    Ok(Value::Record(rec))
                }
                other => Err(AlgError::runtime(
                    span,
                    format!(
                        "Incompatibilidade de tipos: esperado '{}', mas recebeu '{}'.",
                        spec.type_label(),
                        other.type_label()
                    ),
                )),
            },
            ResolvedType::Array { .. } => match self {
                Value::Array(arr)
                    if ResolvedType::Array {
                        elem_type: Box::new(arr.elem_type.clone()),
                        dims: arr.dims.clone(),
                    }
                    .matches(target) =>
                {
                    Ok(Value::Array(arr))
                }
                other => Err(AlgError::runtime(
                    span,
                    format!(
                        "Incompatibilidade de tipos: esperado '{}', mas recebeu '{}'.",
                        target.type_label(),
                        other.type_label()
                    ),
                )),
            },
        }
    }

    fn snapshot_leaf_count(&self) -> usize {
        match self {
            Value::Array(arr) => arr
                .elements
                .len()
                .saturating_mul(arr.elem_type.snapshot_leaf_count()),
            Value::Record(rec) => rec
                .fields
                .iter()
                .fold(0usize, |acc, v| acc.saturating_add(v.snapshot_leaf_count())),
            _ => 1,
        }
    }

    pub fn snapshot_details(&self, limit: usize) -> (Vec<VarArrayItem>, Option<usize>) {
        match self {
            Value::Array(_) | Value::Record(_) => {
                let total = self.snapshot_leaf_count();
                let mut items = Vec::with_capacity(limit.min(total));
                collect_value_snapshot_items(self, "", &mut items, limit);
                (items, Some(total))
            }
            _ => (Vec::new(), None),
        }
    }

    pub fn as_number(&self, span: Span) -> Result<f64> {
        match self {
            Value::Inteiro(n) => Ok(*n as f64),
            Value::Real(f) => Ok(*f),
            other => Err(AlgError::runtime(
                Some(span),
                format!(
                    "Esperado valor numérico ('inteiro' ou 'real'), mas recebeu '{}'.",
                    other.type_label()
                ),
            )),
        }
    }

    pub fn as_int(&self, span: Span) -> Result<i64> {
        match self {
            Value::Inteiro(n) => Ok(*n),
            other => Err(AlgError::runtime(
                Some(span),
                format!("Esperado valor 'inteiro', mas recebeu '{}'.", other.type_label()),
            )),
        }
    }

    pub fn as_bool(&self, span: Span) -> Result<bool> {
        match self {
            Value::Logico(b) => Ok(*b),
            other => Err(AlgError::runtime(
                Some(span),
                format!("Esperado valor 'logico', mas recebeu '{}'.", other.type_label()),
            )),
        }
    }

    pub fn as_str(&self, span: Span) -> Result<&str> {
        match self {
            Value::Caractere(s) => Ok(s.as_str()),
            other => Err(AlgError::runtime(
                Some(span),
                format!("Esperado valor 'caractere', mas recebeu '{}'.", other.type_label()),
            )),
        }
    }

    pub fn to_display_string(&self) -> String {
        match self {
            Value::Inteiro(n) => n.to_string(),
            Value::Real(f) => format_real_default(*f),
            Value::Caractere(s) => s.clone(),
            Value::Logico(b) => {
                if *b {
                    "VERDADEIRO".to_string()
                } else {
                    "FALSO".to_string()
                }
            }
            Value::Record(rec) => rec.preview(),
            Value::Array(arr) => arr.preview(),
        }
    }

    pub fn format_write(
        &self,
        width: Option<i64>,
        decimals: Option<i64>,
        span: Span,
    ) -> Result<String> {
        let w = match width {
            Some(raw_w) => {
                if raw_w > MAX_FORMAT_WIDTH as i64 {
                    return Err(AlgError::runtime(
                        Some(span),
                        format!(
                            "Largura de formatação ({raw_w}) excede o limite máximo ({MAX_FORMAT_WIDTH})."
                        ),
                    ));
                }
                raw_w.max(0) as usize
            }
            None => 0,
        };

        if let Some(dec) = decimals {
            if dec < 0 {
                return Err(AlgError::runtime(
                    Some(span),
                    "Número de casas decimais na formatação não pode ser negativo.",
                ));
            }
            if dec > MAX_FORMAT_DECIMALS as i64 {
                return Err(AlgError::runtime(
                    Some(span),
                    format!(
                        "Número de casas decimais ({dec}) excede o limite máximo ({MAX_FORMAT_DECIMALS})."
                    ),
                ));
            }
            let num = self.as_number(span)?;
            let mut fixed = format!("{:.prec$}", clean_neg_zero(num), prec = dec as usize);
            if fixed.starts_with('-') && fixed[1..].bytes().all(|b| b == b'0' || b == b'.') {
                fixed.remove(0);
            }
            return Ok(pad_left(&fixed, w));
        }
        let raw = self.to_display_string();
        Ok(pad_left(&raw, w))
    }
}

fn clean_neg_zero(f: f64) -> f64 {
    if f == 0.0 {
        0.0
    } else {
        f
    }
}

pub fn format_real_default(value: f64) -> String {
    let v = clean_neg_zero(value);
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v.is_sign_positive() {
            "Infinito".to_string()
        } else {
            "-Infinito".to_string()
        };
    }
    if v == 0.0 {
        return "0".to_string();
    }
    // Reduz ruído típico de ponto flutuante (ex.: 0.1 + 0.2) preservando valores muito pequenos (ex.: 1e-13)
    if v.abs() >= 1e-9 {
        let rounded = format!("{v:.12}");
        if let Ok(parsed) = rounded.parse::<f64>() {
            let p = clean_neg_zero(parsed);
            if p.fract() == 0.0 && p.abs() < 1e15 {
                return format!("{p:.0}");
            }
            return format!("{p}");
        }
    }
    format!("{v}")
}

fn pad_left(text: &str, width: usize) -> String {
    let capped_width = width.min(MAX_FORMAT_WIDTH);
    let len = text.chars().count();
    if len >= capped_width {
        text.to_string()
    } else {
        format!("{}{}", " ".repeat(capped_width - len), text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarArrayItem {
    pub index_label: String,
    pub value_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarSnapshot {
    pub name: String,
    pub type_label: String,
    pub value_text: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<VarArrayItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_items: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_range_len_guards_extreme_bounds_without_truncation() {
        assert_eq!(checked_range_len(1, 5).unwrap(), 5);
        assert_eq!(checked_range_len(-2, 2).unwrap(), 5);
        assert_eq!(checked_range_len(i64::MIN, i64::MIN).unwrap(), 1);
        assert!(checked_range_len(5, 1).is_err());
        assert!(checked_range_len(i64::MIN, i64::MAX).is_err());

        let span = Span::new(1, 1, 1);
        let huge_ty = ResolvedType::Array {
            elem_type: Box::new(ResolvedType::Primitive(PrimitiveType::Inteiro)),
            dims: vec![ResolvedArrayRange {
                start: i64::MIN,
                end: i64::MAX,
            }],
        };
        assert!(huge_ty.checked_leaf_count().is_err());
        assert_eq!(huge_ty.leaf_count(), usize::MAX);
        assert!(ArrayValue::new(
            "v",
            ResolvedType::Primitive(PrimitiveType::Inteiro),
            vec![ResolvedArrayRange {
                start: i64::MIN,
                end: i64::MAX
            }],
            span,
        )
        .is_err());
    }

    #[test]
    fn format_real_preserves_small_values_and_strips_neg_zero() {
        assert_eq!(format_real_default(-0.0), "0");
        assert_eq!(format_real_default(0.1 + 0.2), "0.3");
        assert_eq!(format_real_default(1e-13), "0.0000000000001");

        let span = Span::new(1, 1, 1);
        assert_eq!(
            Value::Real(-0.001)
                .format_write(Some(5), Some(2), span)
                .unwrap(),
            " 0.00"
        );
        assert_eq!(
            Value::Real(-0.1)
                .format_write(Some(3), Some(0), span)
                .unwrap(),
            "  0"
        );
        assert!(Value::Inteiro(1)
            .format_write(Some(MAX_FORMAT_WIDTH as i64 + 1), None, span)
            .is_err());
        assert!(Value::Real(1.5)
            .format_write(Some(5), Some(MAX_FORMAT_DECIMALS as i64 + 1), span)
            .is_err());
    }

    #[test]
    fn snapshot_details_caps_items_and_computes_exact_total() {
        let span = Span::new(1, 1, 1);
        let arr = ArrayValue::new(
            "grande",
            ResolvedType::Primitive(PrimitiveType::Inteiro),
            vec![ResolvedArrayRange {
                start: 1,
                end: 10_000,
            }],
            span,
        )
        .unwrap();
        let val = Value::Array(Box::new(arr));
        let (items, total) = val.snapshot_details(10);
        assert_eq!(items.len(), 10);
        assert_eq!(total, Some(10_000));
        assert_eq!(items[0].index_label, "[1]");
        assert_eq!(items[9].index_label, "[10]");
    }

    #[test]
    fn ensure_string_len_enforces_max_string_bytes() {
        let span = Span::new(1, 1, 1);
        assert!(ensure_string_len(MAX_STRING_BYTES, Some(span)).is_ok());
        let err = ensure_string_len(MAX_STRING_BYTES + 1, Some(span)).unwrap_err();
        assert!(err.message.contains("Tamanho máximo de texto"));
    }
}
