use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Posição de um token ou erro no código-fonte (base 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub line: usize,
    pub column: usize,
    pub length: usize,
}

impl Span {
    pub fn new(line: usize, column: usize, length: usize) -> Self {
        Self {
            line,
            column,
            length: length.max(1),
        }
    }
}

/// Categoria do erro para destaque visual na IDE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Lexico,
    Sintatico,
    Semantico,
    Execucao,
    Interrompido,
}

#[derive(Debug, Clone, Error, PartialEq, Eq, Serialize, Deserialize)]
#[error("[Linha {span_line}:{span_col}] {message}", span_line = .span.as_ref().map(|s| s.line).unwrap_or(0), span_col = .span.as_ref().map(|s| s.column).unwrap_or(0))]
pub struct AlgError {
    pub kind: ErrorKind,
    pub message: String,
    pub span: Option<Span>,
}

impl AlgError {
    pub fn lex(span: Span, message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Lexico,
            message: message.into(),
            span: Some(span),
        }
    }

    pub fn parse(span: Span, message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Sintatico,
            message: message.into(),
            span: Some(span),
        }
    }

    pub fn semantic(span: Span, message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Semantico,
            message: message.into(),
            span: Some(span),
        }
    }

    pub fn runtime(span: Option<Span>, message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Execucao,
            message: message.into(),
            span,
        }
    }

    pub fn interrupted() -> Self {
        Self {
            kind: ErrorKind::Interrompido,
            message: "Execução interrompida pelo usuário.".to_string(),
            span: None,
        }
    }
}

pub type Result<T> = std::result::Result<T, AlgError>;
