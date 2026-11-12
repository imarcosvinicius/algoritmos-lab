use super::Parser;
use crate::ast::{
    AccessStep, AleatorioConfig, CaseArm, CaseMatcher, LValue, Stmt, TimerConfig, WriteArg,
};
use crate::error::{Result, AlgError};
use crate::lexer::normalize_keyword;
use crate::token::{Keyword, TokenKind};

impl<'a> Parser<'a> {
    pub(super) fn parse_stmt_list(&mut self, stop_keywords: &[Keyword]) -> Result<Vec<Stmt>> {
        let mut stmts = Vec::new();
        loop {
            self.skip_separators();
            if matches!(self.current().kind, TokenKind::Eof) {
                break;
            }
            if let TokenKind::Keyword(kw) = &self.current().kind {
                if stop_keywords.contains(kw) || self.is_block_terminator() {
                    break;
                }
            }
            stmts.push(self.parse_stmt()?);
        }
        Ok(stmts)
    }

    pub(super) fn parse_stmt(&mut self) -> Result<Stmt> {
        let span = self.current().span;
        self.with_depth(span, |p| p.parse_stmt_inner())
    }

    fn parse_stmt_inner(&mut self) -> Result<Stmt> {
        let tok = self.current().clone();
        match &tok.kind {
            TokenKind::Keyword(Keyword::Escreva) | TokenKind::Keyword(Keyword::Escreval) => {
                let newline = matches!(tok.kind, TokenKind::Keyword(Keyword::Escreval));
                self.advance();
                let mut args = Vec::new();
                if matches!(self.current().kind, TokenKind::LParen) {
                    self.advance();
                    if !matches!(self.current().kind, TokenKind::RParen) {
                        loop {
                            let expr = self.parse_expr()?;
                            let mut width = None;
                            let mut decimals = None;
                            if matches!(self.current().kind, TokenKind::Colon) {
                                self.advance();
                                width = Some(self.parse_expr()?);
                                if matches!(self.current().kind, TokenKind::Colon) {
                                    self.advance();
                                    decimals = Some(self.parse_expr()?);
                                }
                            }
                            args.push(WriteArg {
                                expr,
                                width,
                                decimals,
                            });
                            if matches!(self.current().kind, TokenKind::Comma) {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect_token(TokenKind::RParen, ")")?;
                } else if !newline {
                    return Err(AlgError::parse(
                        tok.span,
                        "Esperado '(' após 'escreva'.",
                    ));
                }
                self.consume_stmt_separators()?;
                Ok(Stmt::Write {
                    args,
                    newline,
                    span: tok.span,
                })
            }
            TokenKind::Keyword(Keyword::Leia) => {
                self.advance();
                self.expect_token(TokenKind::LParen, "(")?;
                let mut targets = Vec::new();
                loop {
                    targets.push(self.parse_lvalue()?);
                    if matches!(self.current().kind, TokenKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect_token(TokenKind::RParen, ")")?;
                self.consume_stmt_separators()?;
                Ok(Stmt::Read {
                    targets,
                    span: tok.span,
                })
            }
            TokenKind::Keyword(Keyword::Limpatela) => {
                self.advance();
                if matches!(self.current().kind, TokenKind::LParen) {
                    self.advance();
                    self.expect_token(TokenKind::RParen, ")")?;
                }
                self.consume_stmt_separators()?;
                Ok(Stmt::ClearScreen { span: tok.span })
            }
            TokenKind::Keyword(Keyword::Se) => self.parse_if(),
            TokenKind::Keyword(Keyword::Escolha) => self.parse_switch(),
            TokenKind::Keyword(Keyword::Para) => self.parse_for(),
            TokenKind::Keyword(Keyword::Enquanto) => self.parse_while(),
            TokenKind::Keyword(Keyword::Repita) => self.parse_repeat(),
            TokenKind::Keyword(Keyword::Interrompa) => {
                self.advance();
                self.consume_stmt_separators()?;
                Ok(Stmt::Break { span: tok.span })
            }
            TokenKind::Keyword(Keyword::Retorne) => {
                self.advance();
                let value = if self.is_stmt_end() {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.consume_stmt_separators()?;
                Ok(Stmt::Return {
                    value,
                    span: tok.span,
                })
            }
            TokenKind::Keyword(Keyword::Arquivo) => {
                let (path, span) = self.parse_arquivo_directive(false)?;
                Ok(Stmt::Arquivo { path, span })
            }
            TokenKind::Keyword(Keyword::Aleatorio) => self.parse_aleatorio(),
            TokenKind::Keyword(Keyword::Timer) => self.parse_timer(),
            TokenKind::Keyword(Keyword::Cronometro) => {
                let enabled = self.parse_on_off_flag_stmt("cronometro")?;
                Ok(Stmt::Cronometro {
                    enabled,
                    span: tok.span,
                })
            }
            TokenKind::Keyword(Keyword::Eco) => {
                let enabled = self.parse_on_off_flag_stmt("eco")?;
                Ok(Stmt::Eco {
                    enabled,
                    span: tok.span,
                })
            }
            TokenKind::Keyword(Keyword::Pausa) => {
                self.advance();
                if matches!(self.current().kind, TokenKind::LParen) {
                    self.advance();
                    self.expect_token(TokenKind::RParen, ")")?;
                }
                self.consume_stmt_separators()?;
                Ok(Stmt::Pausa { span: tok.span })
            }
            TokenKind::Keyword(Keyword::Debug) => {
                self.advance();
                let cond = if self.is_stmt_end() {
                    None
                } else if matches!(self.current().kind, TokenKind::LParen)
                    && matches!(self.peek(1).kind, TokenKind::RParen)
                {
                    self.advance();
                    self.advance();
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.consume_stmt_separators()?;
                Ok(Stmt::Debug {
                    cond,
                    span: tok.span,
                })
            }
            TokenKind::Keyword(Keyword::Mudacor) => self.parse_mudacor(),
            TokenKind::Keyword(Keyword::Dos) => {
                self.advance();
                if matches!(self.current().kind, TokenKind::LParen) {
                    self.advance();
                    let _ = self.parse_arg_list()?;
                    self.expect_token(TokenKind::RParen, ")")?;
                } else if !self.is_stmt_end() {
                    loop {
                        let _ = self.parse_expr()?;
                        if matches!(self.current().kind, TokenKind::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.consume_stmt_separators()?;
                Ok(Stmt::Dos { span: tok.span })
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                if matches!(self.peek(1).kind, TokenKind::LParen) {
                    self.advance();
                    self.advance();
                    let args = self.parse_arg_list()?;
                    self.expect_token(TokenKind::RParen, ")")?;
                    self.consume_stmt_separators()?;
                    Ok(Stmt::ProcCall {
                        name,
                        args,
                        span: tok.span,
                    })
                } else if matches!(
                    self.peek(1).kind,
                    TokenKind::Assign | TokenKind::LBracket | TokenKind::Dot
                ) {
                    let target = self.parse_lvalue()?;
                    self.expect_token(TokenKind::Assign, "<-")?;
                    let value = self.parse_expr()?;
                    self.consume_stmt_separators()?;
                    Ok(Stmt::Assign {
                        target,
                        value,
                        span: tok.span,
                    })
                } else if matches!(
                    self.peek(1).kind,
                    TokenKind::Newline | TokenKind::Semicolon | TokenKind::Eof
                ) || Self::is_block_terminator_kind(&self.peek(1).kind)
                {
                    // Chamada de procedimento sem parênteses (ex.: `menu`)
                    self.advance();
                    self.consume_stmt_separators()?;
                    Ok(Stmt::ProcCall {
                        name,
                        args: Vec::new(),
                        span: tok.span,
                    })
                } else {
                    Err(AlgError::parse(
                        tok.span,
                        format!(
                            "Comando inválido iniciando com '{}'. Para atribuir, use '<-'.",
                            tok.lexeme
                        ),
                    ))
                }
            }
            _ => Err(AlgError::parse(
                tok.span,
                format!("Comando inesperado: '{}'.", tok.lexeme),
            )),
        }
    }

    fn try_consume_on_off(&mut self) -> Option<bool> {
        if let TokenKind::Ident(word) = &self.current().kind {
            match normalize_keyword(word).as_str() {
                "on" => {
                    self.advance();
                    Some(true)
                }
                "off" => {
                    self.advance();
                    Some(false)
                }
                _ => None,
            }
        } else {
            None
        }
    }

    fn parse_on_off_flag_stmt(&mut self, cmd_name: &str) -> Result<bool> {
        self.advance();
        let has_paren = matches!(self.current().kind, TokenKind::LParen);
        if has_paren {
            self.advance();
        }
        let enabled = self.try_consume_on_off().ok_or_else(|| {
            AlgError::parse(
                self.current().span,
                format!("Esperado 'on' ou 'off' após '{cmd_name}'."),
            )
        })?;
        if has_paren {
            self.expect_token(TokenKind::RParen, ")")?;
        }
        self.consume_stmt_separators()?;
        Ok(enabled)
    }

    fn parse_range_or_max_args(&mut self) -> Result<AleatorioConfig> {
        let first = self.parse_expr()?;
        if matches!(self.current().kind, TokenKind::Comma) {
            self.advance();
            let second = self.parse_expr()?;
            Ok(AleatorioConfig::Range(first, second))
        } else {
            Ok(AleatorioConfig::MaxOnly(first))
        }
    }

    fn parse_aleatorio(&mut self) -> Result<Stmt> {
        let start = self.advance();
        let config = if self.is_stmt_end() {
            AleatorioConfig::OnDefault
        } else if matches!(self.current().kind, TokenKind::LParen) {
            let saved = self.pos;
            let attempt = (|| -> Result<AleatorioConfig> {
                self.advance();
                if matches!(self.current().kind, TokenKind::RParen) {
                    self.advance();
                    Ok(AleatorioConfig::OnDefault)
                } else if let Some(flag) = self.try_consume_on_off() {
                    self.expect_token(TokenKind::RParen, ")")?;
                    Ok(if flag {
                        AleatorioConfig::OnDefault
                    } else {
                        AleatorioConfig::Off
                    })
                } else {
                    let cfg = self.parse_range_or_max_args()?;
                    self.expect_token(TokenKind::RParen, ")")?;
                    Ok(cfg)
                }
            })();
            match attempt {
                Ok(cfg) if self.is_stmt_end() => cfg,
                _ => {
                    self.pos = saved;
                    self.parse_range_or_max_args()?
                }
            }
        } else if let Some(flag) = self.try_consume_on_off() {
            if flag {
                AleatorioConfig::OnDefault
            } else {
                AleatorioConfig::Off
            }
        } else {
            self.parse_range_or_max_args()?
        };
        self.consume_stmt_separators()?;
        Ok(Stmt::Aleatorio {
            config,
            span: start.span,
        })
    }

    fn parse_timer(&mut self) -> Result<Stmt> {
        let start = self.advance();
        if self.is_stmt_end() {
            return Err(AlgError::parse(
                start.span,
                "Esperado 'on', 'off' ou tempo em milissegundos após 'timer'.",
            ));
        }
        let config = if matches!(self.current().kind, TokenKind::LParen) {
            let saved = self.pos;
            let attempt = (|| -> Result<TimerConfig> {
                self.advance();
                if let Some(flag) = self.try_consume_on_off() {
                    self.expect_token(TokenKind::RParen, ")")?;
                    Ok(if flag {
                        TimerConfig::On
                    } else {
                        TimerConfig::Off
                    })
                } else {
                    let expr = self.parse_expr()?;
                    self.expect_token(TokenKind::RParen, ")")?;
                    Ok(TimerConfig::Millis(expr))
                }
            })();
            match attempt {
                Ok(cfg) if self.is_stmt_end() => cfg,
                _ => {
                    self.pos = saved;
                    TimerConfig::Millis(self.parse_expr()?)
                }
            }
        } else if let Some(flag) = self.try_consume_on_off() {
            if flag {
                TimerConfig::On
            } else {
                TimerConfig::Off
            }
        } else {
            TimerConfig::Millis(self.parse_expr()?)
        };
        self.consume_stmt_separators()?;
        Ok(Stmt::Timer {
            config,
            span: start.span,
        })
    }

    fn parse_mudacor(&mut self) -> Result<Stmt> {
        let start = self.advance();
        let has_paren = matches!(self.current().kind, TokenKind::LParen);
        if has_paren {
            self.advance();
        }
        let color = self.parse_expr()?;
        self.expect_token(TokenKind::Comma, ",")?;
        let target = self.parse_expr()?;
        if has_paren {
            self.expect_token(TokenKind::RParen, ")")?;
        }
        self.consume_stmt_separators()?;
        Ok(Stmt::MudaCor {
            color,
            target,
            span: start.span,
        })
    }

    fn parse_lvalue(&mut self) -> Result<LValue> {
        let (base, base_span) = self.expect_ident("variável")?;
        let mut steps = Vec::new();
        loop {
            match &self.current().kind {
                TokenKind::LBracket => {
                    let (indices, lb) = self.parse_bracket_indices()?;
                    steps.push(AccessStep::Index { indices, span: lb });
                }
                TokenKind::Dot => {
                    self.advance();
                    let (field, fspan) = self.expect_ident("campo do registro")?;
                    steps.push(AccessStep::Field {
                        name: field,
                        span: fspan,
                    });
                }
                _ => break,
            }
        }
        Ok(LValue {
            base,
            base_span,
            steps,
            span: base_span,
        })
    }

    fn parse_if(&mut self) -> Result<Stmt> {
        let start = self.expect_keyword(Keyword::Se, "se")?;
        let cond = self.parse_expr()?;
        self.expect_keyword(Keyword::Entao, "entao")?;
        self.skip_separators();

        let then_body = self.parse_stmt_list(&[Keyword::Senao, Keyword::Fimse])?;
        let else_body = if matches!(self.current().kind, TokenKind::Keyword(Keyword::Senao)) {
            self.advance();
            self.skip_separators();
            self.parse_stmt_list(&[Keyword::Fimse])?
        } else {
            Vec::new()
        };

        self.expect_keyword(
            Keyword::Fimse,
            &format!("fimse (para fechar o 'se' da linha {})", start.span.line),
        )?;
        self.consume_stmt_separators()?;
        Ok(Stmt::If {
            cond,
            then_body,
            else_body,
            span: start.span,
        })
    }

    fn parse_switch(&mut self) -> Result<Stmt> {
        let start = self.expect_keyword(Keyword::Escolha, "escolha")?;
        let expr = self.parse_expr()?;
        self.skip_separators();

        let mut arms = Vec::new();
        while matches!(self.current().kind, TokenKind::Keyword(Keyword::Caso)) {
            let caso_tok = self.advance();
            let mut matchers = Vec::new();
            loop {
                let first = self.parse_expr()?;
                if matches!(
                    self.current().kind,
                    TokenKind::Keyword(Keyword::Ate) | TokenKind::DotDot
                ) {
                    self.advance();
                    let end = self.parse_expr()?;
                    matchers.push(CaseMatcher::Range(first, end));
                } else {
                    matchers.push(CaseMatcher::Value(first));
                }
                if matches!(self.current().kind, TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            if matches!(self.current().kind, TokenKind::Colon) {
                self.advance();
            }
            self.skip_separators();
            let body = self.parse_stmt_list(&[
                Keyword::Caso,
                Keyword::Outrocaso,
                Keyword::Fimescolha,
            ])?;
            arms.push(CaseArm {
                matchers,
                body,
                span: caso_tok.span,
            });
        }

        let otherwise = if matches!(self.current().kind, TokenKind::Keyword(Keyword::Outrocaso)) {
            self.advance();
            if matches!(self.current().kind, TokenKind::Colon) {
                self.advance();
            }
            self.skip_separators();
            self.parse_stmt_list(&[Keyword::Fimescolha])?
        } else {
            Vec::new()
        };

        self.expect_keyword(
            Keyword::Fimescolha,
            &format!(
                "fimescolha (para fechar o 'escolha' da linha {})",
                start.span.line
            ),
        )?;
        self.consume_stmt_separators()?;

        Ok(Stmt::Switch {
            expr,
            arms,
            otherwise,
            span: start.span,
        })
    }

    fn parse_for(&mut self) -> Result<Stmt> {
        let start_tok = self.expect_keyword(Keyword::Para, "para")?;
        let (var, _) = self.expect_ident("variável de controle")?;
        self.expect_keyword(Keyword::De, "de")?;
        let start = self.parse_expr()?;
        self.expect_keyword(Keyword::Ate, "ate")?;
        let end = self.parse_expr()?;
        let step = if matches!(self.current().kind, TokenKind::Keyword(Keyword::Passo)) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect_keyword(Keyword::Faca, "faca")?;
        self.skip_separators();

        let body = self.parse_stmt_list(&[Keyword::Fimpara])?;
        self.expect_keyword(
            Keyword::Fimpara,
            &format!(
                "fimpara (para fechar o 'para' da linha {})",
                start_tok.span.line
            ),
        )?;
        self.consume_stmt_separators()?;

        Ok(Stmt::For {
            var,
            start,
            end,
            step,
            body,
            span: start_tok.span,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt> {
        let start = self.expect_keyword(Keyword::Enquanto, "enquanto")?;
        let cond = self.parse_expr()?;
        self.expect_keyword(Keyword::Faca, "faca")?;
        self.skip_separators();

        let body = self.parse_stmt_list(&[Keyword::Fimenquanto])?;
        self.expect_keyword(
            Keyword::Fimenquanto,
            &format!(
                "fimenquanto (para fechar o 'enquanto' da linha {})",
                start.span.line
            ),
        )?;
        self.consume_stmt_separators()?;

        Ok(Stmt::While {
            cond,
            body,
            span: start.span,
        })
    }

    fn parse_repeat(&mut self) -> Result<Stmt> {
        let start = self.expect_keyword(Keyword::Repita, "repita")?;
        self.skip_separators();

        let body = self.parse_stmt_list(&[Keyword::Ate])?;
        self.expect_keyword(
            Keyword::Ate,
            &format!("ate (para fechar o 'repita' da linha {})", start.span.line),
        )?;
        let until_cond = self.parse_expr()?;
        self.consume_stmt_separators()?;

        Ok(Stmt::Repeat {
            body,
            until_cond,
            span: start.span,
        })
    }
}
