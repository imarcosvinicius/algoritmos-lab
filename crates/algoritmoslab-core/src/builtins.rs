use crate::error::{Result, Span, AlgError};
use crate::lexer::normalize_keyword;
use crate::value::{ensure_string_len, Value};
use std::borrow::Cow;
use std::f64::consts::PI;

pub struct RngState(u64);

impl RngState {
    pub fn from_time() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self(nanos | 1)
    }

    pub fn from_seed(seed: u64) -> Self {
        Self(if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn next_f64(&mut self) -> f64 {
        // [0.0, 1.0)
        ((self.next_u64() >> 11) as f64) / ((1u64 << 53) as f64)
    }

    pub fn next_range(&mut self, max_exclusive: i64) -> i64 {
        if max_exclusive <= 0 {
            return 0;
        }
        (self.next_u64() % (max_exclusive as u64)) as i64
    }
}

pub fn is_zero_arg_builtin(name: &str) -> bool {
    matches!(normalize_keyword(name).as_str(), "pi" | "rand")
}

pub fn is_builtin_name(name: &str) -> bool {
    matches!(
        normalize_keyword(name).as_str(),
        "abs"
            | "int"
            | "quad"
            | "raizq"
            | "exp"
            | "pot"
            | "log"
            | "logn"
            | "pi"
            | "sen"
            | "cos"
            | "tan"
            | "cotan"
            | "arcsen"
            | "arccos"
            | "arctan"
            | "grauprad"
            | "radpgrau"
            | "rand"
            | "randi"
            | "compr"
            | "maiusc"
            | "minusc"
            | "copia"
            | "pos"
            | "asc"
            | "carac"
            | "numpcarac"
            | "caracpnum"
    )
}

/// Faz o parsing de um número real com suporte à vírgula decimal PT-BR (quando única e sem ponto),
/// rejeitando valores não finitos (`NaN`, `inf`, `infinity`, etc.).
pub fn parse_finite_f64(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized: Cow<'_, str> = if !trimmed.contains('.') && trimmed.matches(',').count() == 1 {
        Cow::Owned(trimmed.replace(',', "."))
    } else {
        Cow::Borrowed(trimmed)
    };
    let f = normalized.parse::<f64>().ok()?;
    if f.is_finite() {
        Some(f)
    } else {
        None
    }
}

fn check_arity(name: &str, args: &[Value], expected: usize, span: Span) -> Result<()> {
    if args.len() != expected {
        Err(AlgError::runtime(
            Some(span),
            format!(
                "Função '{name}' esperava {expected} argumento(s), mas recebeu {}.",
                args.len()
            ),
        ))
    } else {
        Ok(())
    }
}

pub fn ensure_finite(value: f64, context: &str, span: Span) -> Result<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(AlgError::runtime(
            Some(span),
            format!("Resultado inválido (não finito) em '{context}'."),
        ))
    }
}

pub fn eval_pow(base: f64, exponent: f64, context: &str, span: Span) -> Result<f64> {
    if !base.is_finite() || !exponent.is_finite() {
        return Err(AlgError::runtime(
            Some(span),
            format!("Argumento não finito inválido em '{context}'."),
        ));
    }
    ensure_finite(base.powf(exponent), context, span)
}

fn unary_math_fn(
    name: &str,
    args: &[Value],
    span: Span,
    op: impl FnOnce(f64) -> Result<f64>,
) -> Result<Value> {
    check_arity(name, args, 1, span)?;
    let n = args[0].as_number(span)?;
    if !n.is_finite() {
        return Err(AlgError::runtime(
            Some(span),
            format!("Argumento não finito inválido em '{name}'."),
        ));
    }
    let out = op(n)?;
    Ok(Value::Real(ensure_finite(out, name, span)?))
}

fn real_to_checked_i64(f: f64, fn_name: &str, span: Span) -> Result<i64> {
    if !f.is_finite() {
        return Err(AlgError::runtime(
            Some(span),
            format!("Não é possível converter valor não finito em inteiro com '{fn_name}'."),
        ));
    }
    let t = f.trunc();
    // -(i64::MIN as f64) == 2^63, o menor valor >= i64::MAX + 1 representável exatamente em f64
    let min_f = i64::MIN as f64;
    let upper_exclusive = -(i64::MIN as f64);
    if t < min_f || t >= upper_exclusive {
        return Err(AlgError::runtime(
            Some(span),
            format!("Valor {f} fora dos limites de 'inteiro' em '{fn_name}'."),
        ));
    }
    Ok(t as i64)
}

pub fn call_builtin(
    name: &str,
    args: Vec<Value>,
    span: Span,
    rng: &mut RngState,
) -> Result<Option<Value>> {
    let key = normalize_keyword(name);

    let val = match key.as_str() {
        // --- Matemáticas ---
        "abs" => {
            check_arity(name, &args, 1, span)?;
            match &args[0] {
                Value::Inteiro(n) => Value::Inteiro(n.checked_abs().ok_or_else(|| {
                    AlgError::runtime(
                        Some(span),
                        "Estouro de inteiro em 'abs' (valor mínimo não representável como positivo).",
                    )
                })?),
                Value::Real(f) => Value::Real(ensure_finite(f.abs(), name, span)?),
                other => {
                    return Err(AlgError::runtime(
                        Some(span),
                        format!("'abs' espera número, recebeu '{}'.", other.type_label()),
                    ))
                }
            }
        }
        "int" => {
            check_arity(name, &args, 1, span)?;
            match &args[0] {
                Value::Inteiro(n) => Value::Inteiro(*n),
                Value::Real(f) => Value::Inteiro(real_to_checked_i64(*f, "int", span)?),
                other => {
                    return Err(AlgError::runtime(
                        Some(span),
                        format!("'int' espera número, recebeu '{}'.", other.type_label()),
                    ))
                }
            }
        }
        "quad" => {
            check_arity(name, &args, 1, span)?;
            match &args[0] {
                Value::Inteiro(n) => Value::Inteiro(n.checked_mul(*n).ok_or_else(|| {
                    AlgError::runtime(Some(span), "Estouro de inteiro em 'quad'.")
                })?),
                Value::Real(f) => Value::Real(ensure_finite(f * f, "quad", span)?),
                other => {
                    return Err(AlgError::runtime(
                        Some(span),
                        format!("'quad' espera número, recebeu '{}'.", other.type_label()),
                    ))
                }
            }
        }
        "raizq" => unary_math_fn(name, &args, span, |n| {
            if n < 0.0 {
                return Err(AlgError::runtime(
                    Some(span),
                    "Não é possível calcular 'raizq' de número negativo.",
                ));
            }
            Ok(n.sqrt())
        })?,
        "exp" => {
            if args.len() == 1 {
                let x = args[0].as_number(span)?;
                if !x.is_finite() {
                    return Err(AlgError::runtime(
                        Some(span),
                        "Argumento não finito inválido em 'exp'.",
                    ));
                }
                Value::Real(ensure_finite(x.exp(), "exp", span)?)
            } else if args.len() == 2 {
                let base = args[0].as_number(span)?;
                let exponent = args[1].as_number(span)?;
                Value::Real(eval_pow(base, exponent, "exp", span)?)
            } else {
                return Err(AlgError::runtime(
                    Some(span),
                    format!(
                        "Função 'exp' esperava 1 ou 2 argumentos, mas recebeu {}.",
                        args.len()
                    ),
                ));
            }
        }
        "pot" => {
            check_arity(name, &args, 2, span)?;
            let base = args[0].as_number(span)?;
            let exponent = args[1].as_number(span)?;
            Value::Real(eval_pow(base, exponent, "pot", span)?)
        }
        "log" => unary_math_fn(name, &args, span, |n| {
            if n <= 0.0 {
                return Err(AlgError::runtime(
                    Some(span),
                    "Logaritmo ('log') indefinido para valores menores ou iguais a zero.",
                ));
            }
            Ok(n.log10())
        })?,
        "logn" => unary_math_fn(name, &args, span, |n| {
            if n <= 0.0 {
                return Err(AlgError::runtime(
                    Some(span),
                    "Logaritmo natural ('logn') indefinido para valores menores ou iguais a zero.",
                ));
            }
            Ok(n.ln())
        })?,
        "pi" => {
            check_arity(name, &args, 0, span)?;
            Value::Real(PI)
        }
        "sen" => unary_math_fn(name, &args, span, |n| Ok(n.sin()))?,
        "cos" => unary_math_fn(name, &args, span, |n| Ok(n.cos()))?,
        "tan" => unary_math_fn(name, &args, span, |n| Ok(n.tan()))?,
        "cotan" => unary_math_fn(name, &args, span, |n| {
            let t = ensure_finite(n.tan(), "cotan", span)?;
            if t == 0.0 {
                return Err(AlgError::runtime(
                    Some(span),
                    "Cotangente indefinida (tangente zero).",
                ));
            }
            Ok(1.0 / t)
        })?,
        "arcsen" => unary_math_fn(name, &args, span, |n| {
            if !(-1.0..=1.0).contains(&n) {
                return Err(AlgError::runtime(
                    Some(span),
                    "'arcsen' exige argumento entre -1 e 1.",
                ));
            }
            Ok(n.asin())
        })?,
        "arccos" => unary_math_fn(name, &args, span, |n| {
            if !(-1.0..=1.0).contains(&n) {
                return Err(AlgError::runtime(
                    Some(span),
                    "'arccos' exige argumento entre -1 e 1.",
                ));
            }
            Ok(n.acos())
        })?,
        "arctan" => unary_math_fn(name, &args, span, |n| Ok(n.atan()))?,
        "grauprad" => unary_math_fn(name, &args, span, |n| Ok(n.to_radians()))?,
        "radpgrau" => unary_math_fn(name, &args, span, |n| Ok(n.to_degrees()))?,
        "rand" => {
            check_arity(name, &args, 0, span)?;
            Value::Real(rng.next_f64())
        }
        "randi" => {
            check_arity(name, &args, 1, span)?;
            let limit = args[0].as_int(span)?;
            if limit <= 0 {
                return Err(AlgError::runtime(
                    Some(span),
                    "'randi' exige um limite inteiro maior que zero.",
                ));
            }
            Value::Inteiro(rng.next_range(limit))
        }

        // --- Texto e Conversão ---
        "compr" => {
            check_arity(name, &args, 1, span)?;
            let s = args[0].as_str(span)?;
            Value::Inteiro(s.chars().count() as i64)
        }
        "maiusc" => {
            check_arity(name, &args, 1, span)?;
            let s = args[0].as_str(span)?;
            let out = s.to_uppercase();
            ensure_string_len(out.len(), Some(span))?;
            Value::Caractere(out)
        }
        "minusc" => {
            check_arity(name, &args, 1, span)?;
            let s = args[0].as_str(span)?;
            let out = s.to_lowercase();
            ensure_string_len(out.len(), Some(span))?;
            Value::Caractere(out)
        }
        "copia" => {
            check_arity(name, &args, 3, span)?;
            let s = args[0].as_str(span)?;
            let start_1 = args[1].as_int(span)?;
            let count = args[2].as_int(span)?;
            if count <= 0 {
                Value::Caractere(String::new())
            } else {
                let start_0 = usize::try_from(start_1.saturating_sub(1).max(0)).unwrap_or(usize::MAX);
                let take_count = usize::try_from(count).unwrap_or(usize::MAX);
                let taken: String = s.chars().skip(start_0).take(take_count).collect();
                ensure_string_len(taken.len(), Some(span))?;
                Value::Caractere(taken)
            }
        }
        "pos" => {
            check_arity(name, &args, 2, span)?;
            let sub = args[0].as_str(span)?;
            let text = args[1].as_str(span)?;
            if sub.is_empty() {
                Value::Inteiro(0)
            } else if let Some(byte_idx) = text.find(sub) {
                let char_idx = text[..byte_idx].chars().count() as i64 + 1;
                Value::Inteiro(char_idx)
            } else {
                Value::Inteiro(0)
            }
        }
        "asc" => {
            check_arity(name, &args, 1, span)?;
            let s = args[0].as_str(span)?;
            let code = s.chars().next().map(|c| c as i64).unwrap_or(0);
            Value::Inteiro(code)
        }
        "carac" => {
            check_arity(name, &args, 1, span)?;
            let code = args[0].as_int(span)?;
            if code < 0 {
                return Err(AlgError::runtime(
                    Some(span),
                    "'carac' exige código inteiro não negativo.",
                ));
            }
            let ch = u32::try_from(code)
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
                .unwrap_or_default();
            ensure_string_len(ch.len(), Some(span))?;
            Value::Caractere(ch)
        }
        "numpcarac" => {
            check_arity(name, &args, 1, span)?;
            match &args[0] {
                Value::Inteiro(_) | Value::Real(_) => {
                    let out = args[0].to_display_string();
                    ensure_string_len(out.len(), Some(span))?;
                    Value::Caractere(out)
                }
                other => {
                    return Err(AlgError::runtime(
                        Some(span),
                        format!(
                            "'numpcarac' espera número, recebeu '{}'.",
                            other.type_label()
                        ),
                    ))
                }
            }
        }
        "caracpnum" => {
            check_arity(name, &args, 1, span)?;
            let s = args[0].as_str(span)?.trim();
            if let Ok(i) = s.parse::<i64>() {
                Value::Inteiro(i)
            } else if let Some(f) = parse_finite_f64(s) {
                Value::Real(f)
            } else {
                return Err(AlgError::runtime(
                    Some(span),
                    format!("Não foi possível converter '{s}' em número com 'caracpnum'."),
                ));
            }
        }
        _ => return Ok(None),
    };

    Ok(Some(val))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::new(1, 1, 1)
    }

    #[test]
    fn abs_and_copia_handle_i64_min_safely() {
        let mut rng = RngState::from_seed(1);
        assert!(call_builtin("abs", vec![Value::Inteiro(i64::MIN)], span(), &mut rng).is_err());

        let copied = call_builtin(
            "copia",
            vec![
                Value::Caractere("abc".to_string()),
                Value::Inteiro(i64::MIN),
                Value::Inteiro(2),
            ],
            span(),
            &mut rng,
        )
        .unwrap()
        .unwrap();
        assert_eq!(copied, Value::Caractere("ab".to_string()));
    }

    #[test]
    fn supports_pot_and_rejects_non_finite_results() {
        let mut rng = RngState::from_seed(1);
        let res = call_builtin(
            "pot",
            vec![Value::Inteiro(2), Value::Inteiro(5)],
            span(),
            &mut rng,
        )
        .unwrap()
        .unwrap();
        assert_eq!(res, Value::Real(32.0));

        assert!(call_builtin(
            "pot",
            vec![Value::Real(-2.0), Value::Real(0.5)],
            span(),
            &mut rng
        )
        .is_err());
        assert!(call_builtin(
            "exp",
            vec![Value::Real(1000.0)],
            span(),
            &mut rng
        )
        .is_err());
        assert!(call_builtin(
            "log",
            vec![Value::Real(f64::NAN)],
            span(),
            &mut rng
        )
        .is_err());
    }

    #[test]
    fn int_validates_range_without_saturation() {
        let mut rng = RngState::from_seed(1);
        assert_eq!(
            call_builtin("int", vec![Value::Real(-3.9)], span(), &mut rng)
                .unwrap()
                .unwrap(),
            Value::Inteiro(-3)
        );
        assert!(call_builtin("int", vec![Value::Real(1e30)], span(), &mut rng).is_err());
        assert!(call_builtin("int", vec![Value::Real(f64::NAN)], span(), &mut rng).is_err());
        assert!(call_builtin(
            "int",
            vec![Value::Real(-(i64::MIN as f64))],
            span(),
            &mut rng
        )
        .is_err());
    }

    #[test]
    fn caracpnum_normalizes_comma_and_rejects_nan_inf() {
        let mut rng = RngState::from_seed(1);
        assert_eq!(
            call_builtin(
                "caracpnum",
                vec![Value::Caractere("12,5".to_string())],
                span(),
                &mut rng
            )
            .unwrap()
            .unwrap(),
            Value::Real(12.5)
        );
        for bad in ["NaN", "inf", "-infinity", "1e500"] {
            assert!(call_builtin(
                "caracpnum",
                vec![Value::Caractere(bad.to_string())],
                span(),
                &mut rng
            )
            .is_err());
        }
    }

    #[test]
    fn string_builtins_enforce_max_string_bytes() {
        let mut rng = RngState::from_seed(1);
        let huge_eszett = "ß".repeat(crate::value::MAX_STRING_BYTES / 4 + 1);
        // "ß" (2 bytes) -> "SS" (2 bytes), wait: "ß".to_uppercase() is "SS" (same bytes).
        // Use 'ß' (2 bytes) -> "SS" (2 bytes)? No, 'ŉ' (2 bytes) -> "ʼN" (3 bytes), or 'ﬁ' (3 bytes) -> "FI" (2 bytes),
        // or 'ẖ' (3 bytes) -> "H̱" (3 bytes), or 'ΐ' (2 bytes: \u{0390}) -> "\u{0399}\u{0308}\u{0301}" (6 bytes)!
        let _ = huge_eszett;
        let expanding = "\u{0390}".repeat(crate::value::MAX_STRING_BYTES / 4);
        assert!(expanding.len() < crate::value::MAX_STRING_BYTES);
        let err = call_builtin("maiusc", vec![Value::Caractere(expanding)], span(), &mut rng)
            .unwrap_err();
        assert!(err.message.contains("Tamanho máximo de texto"));
    }
}
