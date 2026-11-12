use super::Parser;
use crate::ast::{BinaryOp, Expr, UnaryOp};
use crate::error::{Result, Span, AlgError};
use crate::lexer::normalize_keyword;
use crate::token::{Keyword, TokenKind};

impl<'a> Parser<'a> {
    pub(super) fn parse_arg_list(&mut self) -> Result<Vec<Expr>> {
        let mut args = Vec::new();
        if !matches!(self.current().kind, TokenKind::RParen) {
            loop {
                args.push(self.parse_expr()?);
                if matches!(self.current().kind, TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        Ok(args)
    }

    pub(super) fn parse_bracket_indices(&mut self) -> Result<(Vec<Expr>, Span)> {
        let lb = self.expect_token(TokenKind::LBracket, "[")?.span;
        let mut indices = Vec::new();
        loop {
            indices.push(self.parse_expr()?);
            if matches!(self.current().kind, TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect_token(TokenKind::RBracket, "]")?;
        Ok((indices, lb))
    }

    pub(super) fn parse_expr(&mut self) -> Result<Expr> {
        let span = self.current().span;
        let is_top = self.expr_depth == 0;
        if is_top {
            self.expr_nodes = 0;
        }
        self.expr_depth += 1;
        let res = self.with_depth(span, |p| p.parse_or());
        self.expr_depth -= 1;
        res
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_and()?;
        loop {
            let op = match self.current().kind {
                TokenKind::Keyword(Keyword::Ou) => BinaryOp::Or,
                TokenKind::Keyword(Keyword::Xou) => BinaryOp::Xor,
                _ => break,
            };
            self.advance();
            let right = self.parse_and()?;
            let span = left.span();
            self.count_expr_node(span)?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_rel()?;
        while matches!(self.current().kind, TokenKind::Keyword(Keyword::E)) {
            self.advance();
            let right = self.parse_rel()?;
            let span = left.span();
            self.count_expr_node(span)?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinaryOp::And,
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn parse_rel(&mut self) -> Result<Expr> {
        let mut left = self.parse_add()?;
        while let Some(op) = match self.current().kind {
            TokenKind::Eq => Some(BinaryOp::Eq),
            TokenKind::Neq => Some(BinaryOp::Neq),
            TokenKind::Lt => Some(BinaryOp::Lt),
            TokenKind::Le => Some(BinaryOp::Le),
            TokenKind::Gt => Some(BinaryOp::Gt),
            TokenKind::Ge => Some(BinaryOp::Ge),
            _ => None,
        } {
            self.advance();
            let right = self.parse_add()?;
            let span = left.span();
            self.count_expr_node(span)?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Expr> {
        let mut left = self.parse_mul()?;
        while let Some(op) = match self.current().kind {
            TokenKind::Plus => Some(BinaryOp::Add),
            TokenKind::Minus => Some(BinaryOp::Sub),
            _ => None,
        } {
            self.advance();
            let right = self.parse_mul()?;
            let span = left.span();
            self.count_expr_node(span)?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn is_contextual_div(kind: &TokenKind) -> bool {
        matches!(kind, TokenKind::Ident(word) if normalize_keyword(word) == "div")
    }

    fn parse_mul(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary()?;
        while let Some(op) = match &self.current().kind {
            TokenKind::Star => Some(BinaryOp::Mul),
            TokenKind::Slash => Some(BinaryOp::Div),
            TokenKind::Backslash => Some(BinaryOp::IntDiv),
            kind if Self::is_contextual_div(kind) => Some(BinaryOp::IntDiv),
            TokenKind::Percent | TokenKind::Keyword(Keyword::Mod) => Some(BinaryOp::Mod),
            _ => None,
        } {
            self.advance();
            let right = self.parse_unary()?;
            let span = left.span();
            self.count_expr_node(span)?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        let tok = self.current().clone();
        self.with_depth(tok.span, |p| match tok.kind {
            TokenKind::Plus => {
                p.advance();
                let expr = p.parse_unary()?;
                p.count_expr_node(tok.span)?;
                Ok(Expr::Unary {
                    op: UnaryOp::Plus,
                    expr: Box::new(expr),
                    span: tok.span,
                })
            }
            TokenKind::Minus => {
                p.advance();
                let expr = p.parse_unary()?;
                p.count_expr_node(tok.span)?;
                Ok(Expr::Unary {
                    op: UnaryOp::Minus,
                    expr: Box::new(expr),
                    span: tok.span,
                })
            }
            TokenKind::Keyword(Keyword::Nao) => {
                p.advance();
                let expr = p.parse_unary()?;
                p.count_expr_node(tok.span)?;
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                    span: tok.span,
                })
            }
            _ => p.parse_power(),
        })
    }

    fn parse_power(&mut self) -> Result<Expr> {
        let left = self.parse_postfix()?;
        if matches!(self.current().kind, TokenKind::Caret) {
            self.advance();
            let right = self.parse_unary()?;
            let span = left.span();
            self.count_expr_node(span)?;
            Ok(Expr::Binary {
                left: Box::new(left),
                op: BinaryOp::Pow,
                right: Box::new(right),
                span,
            })
        } else {
            Ok(left)
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            match &self.current().kind {
                TokenKind::LBracket => {
                    let (indices, lb) = self.parse_bracket_indices()?;
                    self.count_expr_node(lb)?;
                    expr = Expr::Index {
                        target: Box::new(expr),
                        indices,
                        span: lb,
                    };
                }
                TokenKind::Dot => {
                    self.advance();
                    let (field, fspan) = self.expect_ident("campo do registro")?;
                    self.count_expr_node(fspan)?;
                    expr = Expr::Field {
                        target: Box::new(expr),
                        field,
                        span: fspan,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        let tok = self.advance();
        self.with_depth(tok.span, |p| {
            p.count_expr_node(tok.span)?;
            match tok.kind {
                TokenKind::IntLit(n) => Ok(Expr::Int(n, tok.span)),
                TokenKind::RealLit(f) => Ok(Expr::Real(f, tok.span)),
                TokenKind::StringLit(s) => Ok(Expr::Str(s, tok.span)),
                TokenKind::Keyword(Keyword::Verdadeiro) => Ok(Expr::Bool(true, tok.span)),
                TokenKind::Keyword(Keyword::Falso) => Ok(Expr::Bool(false, tok.span)),
                TokenKind::Ident(name) => {
                    if matches!(p.current().kind, TokenKind::LParen) {
                        p.advance();
                        let args = p.parse_arg_list()?;
                        p.expect_token(TokenKind::RParen, ")")?;
                        Ok(Expr::Call {
                            name,
                            args,
                            span: tok.span,
                        })
                    } else {
                        Ok(Expr::Var(name, tok.span))
                    }
                }
                TokenKind::LParen => {
                    let expr = p.parse_expr()?;
                    p.expect_token(TokenKind::RParen, ")")?;
                    Ok(expr)
                }
                _ => Err(AlgError::parse(
                    tok.span,
                    format!(
                        "Esperado expressão, mas encontrado '{}'.",
                        if tok.lexeme.is_empty() {
                            "fim do arquivo"
                        } else {
                            &tok.lexeme
                        }
                    ),
                )),
            }
        })
    }
}
