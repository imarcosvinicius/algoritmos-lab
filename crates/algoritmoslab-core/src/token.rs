use crate::error::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum Keyword {
    Algoritmo,
    Fimalgoritmo,
    Var,
    Inicio,
    Const,
    Tipo,
    Registro,
    Fimregistro,
    // Tipos
    Inteiro,
    Real,
    Caractere,
    Logico,
    Vetor,
    De,
    // Controle de fluxo
    Se,
    Entao,
    Senao,
    Fimse,
    Escolha,
    Caso,
    Outrocaso,
    Fimescolha,
    Para,
    Ate,
    Passo,
    Faca,
    Fimpara,
    Enquanto,
    Fimenquanto,
    Repita,
    Interrompa,
    // Sub-rotinas
    Procedimento,
    Fimprocedimento,
    Funcao,
    Fimfuncao,
    Retorne,
    // Operadores em palavra
    E,
    Ou,
    Xou,
    Nao,
    Mod,
    // Literais booleanos
    Verdadeiro,
    Falso,
    // I/O e comandos especiais da linguagem .alg
    Escreva,
    Escreval,
    Leia,
    Limpatela,
    Arquivo,
    Aleatorio,
    Timer,
    Cronometro,
    Pausa,
    Debug,
    Eco,
    Mudacor,
    Dos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Keyword(Keyword),
    Ident(String),
    IntLit(i64),
    RealLit(f64),
    StringLit(String),

    // Operadores e pontuação
    Assign,   // <-
    DotDot,   // ..
    Dot,      // .
    Eq,       // =
    Neq,      // <>
    Lt,       // <
    Le,       // <=
    Gt,       // >
    Ge,       // >=
    Plus,     // +
    Minus,    // -
    Star,     // *
    Slash,    // /
    Backslash,// \
    Caret,    // ^
    Percent,  // %
    LParen,   // (
    RParen,   // )
    LBracket, // [
    RBracket, // ]
    Comma,    // ,
    Colon,    // :
    Semicolon,// ;

    Newline,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub span: Span,
}
