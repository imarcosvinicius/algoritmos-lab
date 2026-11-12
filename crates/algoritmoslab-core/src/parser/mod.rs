mod decl;
mod expr;
mod stmt;

use crate::ast::Program;
use crate::error::{Result, Span, AlgError};
use crate::token::{Keyword, Token, TokenKind};

pub(crate) const MAX_PARSE_DEPTH: usize = 256;
pub(crate) const MAX_EXPR_NODES: usize = 16_384;

pub fn parse_program(tokens: &[Token]) -> Result<Program> {
    let last = tokens.last().ok_or_else(|| {
        AlgError::parse(Span::new(1, 1, 1), "Fluxo de tokens vazio.")
    })?;
    let eof_token = Token {
        kind: TokenKind::Eof,
        lexeme: String::new(),
        span: last.span,
    };
    let mut parser = Parser {
        tokens,
        pos: 0,
        eof_token,
        depth: 0,
        expr_depth: 0,
        expr_nodes: 0,
        has_header_arquivo: false,
    };
    parser.parse_program_inner()
}

pub(crate) struct Parser<'a> {
    tokens: &'a [Token],
    pub(crate) pos: usize,
    eof_token: Token,
    depth: usize,
    pub(crate) expr_depth: usize,
    pub(crate) expr_nodes: usize,
    pub(crate) has_header_arquivo: bool,
}

impl<'a> Parser<'a> {
    pub(crate) fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&self.eof_token)
    }

    pub(crate) fn peek(&self, offset: usize) -> &Token {
        self.tokens
            .get(self.pos.saturating_add(offset))
            .unwrap_or(&self.eof_token)
    }

    pub(crate) fn advance(&mut self) -> Token {
        let tok = self.current().clone();
        if !matches!(tok.kind, TokenKind::Eof) && self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    pub(crate) fn with_depth<T>(
        &mut self,
        span: Span,
        f: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        if self.depth >= MAX_PARSE_DEPTH {
            return Err(AlgError::parse(
                span,
                format!(
                    "Profundidade máxima de aninhamento sintático ({MAX_PARSE_DEPTH}) excedida."
                ),
            ));
        }
        self.depth += 1;
        let res = f(self);
        self.depth -= 1;
        res
    }

    pub(crate) fn count_expr_node(&mut self, span: Span) -> Result<()> {
        self.expr_nodes += 1;
        if self.expr_nodes > MAX_EXPR_NODES {
            return Err(AlgError::parse(
                span,
                format!(
                    "Expressão excede o limite máximo de complexidade ({MAX_EXPR_NODES} nós)."
                ),
            ));
        }
        Ok(())
    }

    pub(crate) fn skip_separators(&mut self) {
        while matches!(
            self.current().kind,
            TokenKind::Newline | TokenKind::Semicolon
        ) {
            self.pos += 1;
        }
    }

    pub(crate) fn is_block_terminator_kind(kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Keyword(
                Keyword::Fimalgoritmo
                    | Keyword::Fimse
                    | Keyword::Senao
                    | Keyword::Caso
                    | Keyword::Outrocaso
                    | Keyword::Fimescolha
                    | Keyword::Fimpara
                    | Keyword::Fimenquanto
                    | Keyword::Ate
                    | Keyword::Fimprocedimento
                    | Keyword::Fimfuncao
            )
        )
    }

    pub(crate) fn is_block_terminator(&self) -> bool {
        Self::is_block_terminator_kind(&self.current().kind)
    }

    pub(crate) fn is_stmt_end(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Newline | TokenKind::Semicolon | TokenKind::Eof
        ) || self.is_block_terminator()
    }

    pub(crate) fn consume_stmt_separators(&mut self) -> Result<()> {
        if self.is_block_terminator() || matches!(self.current().kind, TokenKind::Eof) {
            return Ok(());
        }
        if matches!(
            self.current().kind,
            TokenKind::Newline | TokenKind::Semicolon
        ) {
            self.skip_separators();
            return Ok(());
        }
        let tok = self.current();
        Err(AlgError::parse(
            tok.span,
            format!(
                "Esperado fim de linha ou ';' após o comando, mas encontrado '{}'.",
                tok.lexeme
            ),
        ))
    }

    pub(crate) fn is_decl_section_end(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Keyword(
                Keyword::Inicio
                    | Keyword::Procedimento
                    | Keyword::Funcao
                    | Keyword::Var
                    | Keyword::Const
                    | Keyword::Tipo
                    | Keyword::Fimregistro
            ) | TokenKind::Eof
        )
    }

    pub(crate) fn consume_decl_separators(&mut self) -> Result<()> {
        if self.is_decl_section_end() {
            return Ok(());
        }
        if matches!(
            self.current().kind,
            TokenKind::Newline | TokenKind::Semicolon
        ) {
            self.skip_separators();
            return Ok(());
        }
        let tok = self.current();
        Err(AlgError::parse(
            tok.span,
            format!(
                "Esperado fim de linha ou ';' após a declaração, mas encontrado '{}'.",
                tok.lexeme
            ),
        ))
    }

    pub(crate) fn expect_keyword(&mut self, kw: Keyword, expected_label: &str) -> Result<Token> {
        if matches!(&self.current().kind, TokenKind::Keyword(k) if *k == kw) {
            Ok(self.advance())
        } else {
            let tok = self.current();
            Err(AlgError::parse(
                tok.span,
                format!(
                    "Esperado '{expected_label}', mas encontrado '{}'.",
                    if tok.lexeme.is_empty() {
                        "fim do arquivo"
                    } else {
                        &tok.lexeme
                    }
                ),
            ))
        }
    }

    pub(crate) fn expect_token(&mut self, expected: TokenKind, label: &str) -> Result<Token> {
        if self.current().kind == expected {
            Ok(self.advance())
        } else {
            let tok = self.current();
            Err(AlgError::parse(
                tok.span,
                format!(
                    "Esperado '{label}', mas encontrado '{}'.",
                    if tok.lexeme.is_empty() {
                        "fim do arquivo"
                    } else {
                        &tok.lexeme
                    }
                ),
            ))
        }
    }

    pub(crate) fn expect_ident(&mut self, context: &str) -> Result<(String, Span)> {
        match &self.current().kind {
            TokenKind::Ident(name) => {
                let name = name.clone();
                let span = self.advance().span;
                Ok((name, span))
            }
            _ => {
                let tok = self.current();
                Err(AlgError::parse(
                    tok.span,
                    format!(
                        "Esperado nome de {context}, mas encontrado '{}'.",
                        if tok.lexeme.is_empty() {
                            "fim do arquivo"
                        } else {
                            &tok.lexeme
                        }
                    ),
                ))
            }
        }
    }

    pub(crate) fn parse_ident_list(&mut self, context: &str) -> Result<Vec<(String, Span)>> {
        let mut names = vec![self.expect_ident(context)?];
        while matches!(self.current().kind, TokenKind::Comma) {
            self.advance();
            names.push(self.expect_ident(context)?);
        }
        Ok(names)
    }

    fn parse_program_inner(&mut self) -> Result<Program> {
        self.skip_separators();
        self.expect_keyword(Keyword::Algoritmo, "algoritmo")?;

        let title = match &self.current().kind {
            TokenKind::StringLit(s) => {
                let t = s.clone();
                self.advance();
                t
            }
            TokenKind::Ident(s) => {
                let t = s.clone();
                self.advance();
                t
            }
            _ => {
                let tok = self.current();
                return Err(AlgError::parse(
                    tok.span,
                    "Esperado o título do algoritmo entre aspas após 'algoritmo' (ex.: algoritmo \"MeuPrograma\").",
                ));
            }
        };
        self.skip_separators();

        let mut consts = Vec::new();
        let mut types = Vec::new();
        let mut arquivo = None;
        let mut globals = Vec::new();
        let mut routines = Vec::new();

        loop {
            self.skip_separators();
            match &self.current().kind {
                TokenKind::Keyword(Keyword::Const) => {
                    self.advance();
                    self.skip_separators();
                    let mut parsed = self.parse_const_section()?;
                    consts.append(&mut parsed);
                }
                TokenKind::Keyword(Keyword::Tipo) => {
                    self.advance();
                    self.skip_separators();
                    let mut parsed = self.parse_tipo_section()?;
                    types.append(&mut parsed);
                }
                TokenKind::Keyword(Keyword::Arquivo) => {
                    let (path, span) = self.parse_arquivo_directive(true)?;
                    arquivo = Some((path, span));
                    self.skip_separators();
                }
                TokenKind::Keyword(Keyword::Var) => {
                    self.advance();
                    self.skip_separators();
                    let mut vars = self.parse_var_section(Some(&mut arquivo))?;
                    globals.append(&mut vars);
                }
                TokenKind::Keyword(Keyword::Procedimento) => {
                    routines.push(self.parse_routine(false)?);
                }
                TokenKind::Keyword(Keyword::Funcao) => {
                    routines.push(self.parse_routine(true)?);
                }
                TokenKind::Keyword(Keyword::Inicio) => {
                    self.advance();
                    self.skip_separators();
                    break;
                }
                TokenKind::Eof => {
                    return Err(AlgError::parse(
                        self.current().span,
                        "Bloco 'inicio' não encontrado no algoritmo.",
                    ));
                }
                _ => {
                    let tok = self.current();
                    return Err(AlgError::parse(
                        tok.span,
                        format!(
                            "Esperado 'const', 'tipo', 'var', 'arquivo', 'procedimento', 'funcao' ou 'inicio', mas encontrado '{}'.",
                            tok.lexeme
                        ),
                    ));
                }
            }
        }

        let body = self.parse_stmt_list(&[Keyword::Fimalgoritmo])?;
        let end_tok = self.expect_keyword(Keyword::Fimalgoritmo, "fimalgoritmo")?;

        self.skip_separators();
        if !matches!(self.current().kind, TokenKind::Eof) {
            let extra = self.current();
            return Err(AlgError::parse(
                extra.span,
                format!(
                    "Conteúdo inesperado após 'fimalgoritmo': '{}'.",
                    extra.lexeme
                ),
            ));
        }

        Ok(Program {
            title,
            consts,
            types,
            arquivo,
            globals,
            routines,
            body,
            end_line: end_tok.span.line,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BinaryOp, CaseMatcher, Stmt, VarType};
    use crate::lexer::tokenize;

    #[test]
    fn empty_token_slice_returns_parse_error_without_panic() {
        let err = parse_program(&[]).unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::Sintatico);
    }

    #[test]
    fn supports_contextual_matriz_and_div_identifiers() {
        let src = r#"algoritmo "Contextual"
var
   matriz: matriz[1..2, 1..2] de inteiro
   div: real
inicio
   matriz[1, 1] <- 7 div 2
   div <- matriz[1, 1] + 0.5
fimalgoritmo
"#;
        let toks = tokenize(src).unwrap();
        let prog = parse_program(&toks).unwrap();
        assert_eq!(prog.globals.len(), 2);
        assert!(matches!(prog.globals[0].var_type, VarType::Array { .. }));
        assert_eq!(prog.globals[0].name, "matriz");
        assert_eq!(prog.globals[1].name, "div");
    }

    #[test]
    fn supports_caso_dotdot_ranges() {
        let src = r#"algoritmo "DotDot"
var
   x: inteiro
inicio
   escolha x
      caso 1..5
         x <- 1
      caso -3..-1, 10..20
         x <- 2
   fimescolha
fimalgoritmo
"#;
        let prog = parse_program(&tokenize(src).unwrap()).unwrap();
        match &prog.body[0] {
            Stmt::Switch { arms, .. } => {
                assert!(matches!(arms[0].matchers[0], CaseMatcher::Range(_, _)));
                assert!(matches!(arms[1].matchers[0], CaseMatcher::Range(_, _)));
                assert!(matches!(arms[1].matchers[1], CaseMatcher::Range(_, _)));
            }
            other => panic!("esperava Switch, recebeu {other:?}"),
        }
    }

    #[test]
    fn supports_arquivo_in_global_var_and_rejects_duplicate_or_local() {
        let ok_src = r#"algoritmo "ArqVar"
var
   arquivo "dados.txt"
   x: inteiro
inicio
fimalgoritmo
"#;
        let prog = parse_program(&tokenize(ok_src).unwrap()).unwrap();
        assert_eq!(prog.arquivo.as_ref().map(|(p, _)| p.as_str()), Some("dados.txt"));
        assert_eq!(prog.globals.len(), 1);

        let dup_src = r#"algoritmo "ArqDup"
arquivo "a.txt"
var
   arquivo "b.txt"
inicio
fimalgoritmo
"#;
        assert!(parse_program(&tokenize(dup_src).unwrap()).is_err());

        let local_src = r#"algoritmo "ArqLocal"
procedimento p
var
   arquivo "a.txt"
inicio
fimprocedimento
inicio
fimalgoritmo
"#;
        assert!(parse_program(&tokenize(local_src).unwrap()).is_err());

        let seq_inicio_src = r#"algoritmo "ArqSeqInicio"
inicio
   arquivo "a.txt"
   arquivo("b.txt")
fimalgoritmo
"#;
        let seq_prog = parse_program(&tokenize(seq_inicio_src).unwrap()).unwrap();
        assert!(seq_prog.arquivo.is_none());
        assert!(matches!(seq_prog.body[0], Stmt::Arquivo { .. }));
        assert!(matches!(seq_prog.body[1], Stmt::Arquivo { .. }));
    }

    #[test]
    fn rejects_tokens_after_fimalgoritmo_and_missing_decl_separators() {
        let trailing = "algoritmo \"t\"\ninicio\nfimalgoritmo\nx <- 1\n";
        assert!(parse_program(&tokenize(trailing).unwrap()).is_err());

        let comments_only = "algoritmo \"t\"\ninicio\nfimalgoritmo\n// ok\n;\n";
        assert!(parse_program(&tokenize(comments_only).unwrap()).is_ok());

        let missing_sep_var = "algoritmo \"t\"\nvar x: inteiro y: real\ninicio\nfimalgoritmo\n";
        assert!(parse_program(&tokenize(missing_sep_var).unwrap()).is_err());

        let missing_sep_const = "algoritmo \"t\"\nconst A = 1 B = 2\ninicio\nfimalgoritmo\n";
        assert!(parse_program(&tokenize(missing_sep_const).unwrap()).is_err());
    }

    #[test]
    fn rejects_deep_nesting_and_long_expr_chains() {
        let deep_parens = format!(
            "algoritmo \"d\"\nvar x: inteiro\ninicio\n   x <- {}1{}\nfimalgoritmo\n",
            "(".repeat(300),
            ")".repeat(300)
        );
        assert!(parse_program(&tokenize(&deep_parens).unwrap()).is_err());

        let long_chain = format!(
            "algoritmo \"c\"\nvar x: inteiro\ninicio\n   x <- 1{}\nfimalgoritmo\n",
            " + 1".repeat(MAX_EXPR_NODES + 10)
        );
        assert!(parse_program(&tokenize(&long_chain).unwrap()).is_err());
    }

    #[test]
    fn x_mod_2_does_not_misparse_as_zero_arg_call() {
        let err = parse_program(&tokenize("algoritmo \"m\"\ninicio\n   x mod 2\nfimalgoritmo\n").unwrap())
            .unwrap_err();
        assert!(err.message.contains("Comando inválido"));
    }

    #[test]
    fn power_is_right_associative() {
        let prog = parse_program(
            &tokenize("algoritmo \"p\"\nvar x: real\ninicio\n   x <- 2 ^ 3 ^ 2\nfimalgoritmo\n")
                .unwrap(),
        )
        .unwrap();
        match &prog.body[0] {
            Stmt::Assign {
                value: crate::ast::Expr::Binary { op: BinaryOp::Pow, right, .. },
                ..
            } => {
                assert!(matches!(
                    right.as_ref(),
                    crate::ast::Expr::Binary {
                        op: BinaryOp::Pow,
                        ..
                    }
                ));
            }
            other => panic!("AST inesperado: {other:?}"),
        }
    }
}
