use super::Parser;
use crate::ast::{
    ArrayRangeExpr, ConstDecl, FieldDecl, ParamDecl, PrimitiveType, RoutineDecl, TypeDecl, VarDecl,
    VarType,
};
use crate::error::{Result, Span, AlgError};
use crate::lexer::normalize_keyword;
use crate::token::{Keyword, TokenKind};

impl<'a> Parser<'a> {
    pub(super) fn is_contextual_matriz_start(&self) -> bool {
        matches!(
            (&self.current().kind, &self.peek(1).kind),
            (TokenKind::Ident(word), TokenKind::LBracket) if normalize_keyword(word) == "matriz"
        )
    }

    fn is_var_type_start(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Keyword(
                Keyword::Registro
                    | Keyword::Vetor
                    | Keyword::Inteiro
                    | Keyword::Real
                    | Keyword::Caractere
                    | Keyword::Logico
            )
        ) || self.is_contextual_matriz_start()
    }

    pub(super) fn parse_const_section(&mut self) -> Result<Vec<ConstDecl>> {
        let mut decls = Vec::new();
        while !self.is_decl_section_end() {
            let (name, span) = self.expect_ident("constante")?;
            self.expect_token(TokenKind::Eq, "=")?;
            let value = self.parse_expr()?;
            decls.push(ConstDecl { name, value, span });
            self.consume_decl_separators()?;
        }
        Ok(decls)
    }

    pub(super) fn parse_tipo_section(&mut self) -> Result<Vec<TypeDecl>> {
        let mut decls = Vec::new();
        while !self.is_decl_section_end() {
            let (name, span) = self.expect_ident("tipo personalizado")?;
            if matches!(self.current().kind, TokenKind::Eq) {
                self.advance();
            } else if !self.is_var_type_start() {
                self.expect_token(TokenKind::Eq, "=")?;
            }
            self.skip_separators();
            let def = self.parse_var_type()?;
            decls.push(TypeDecl { name, def, span });
            self.consume_decl_separators()?;
        }
        Ok(decls)
    }

    pub(super) fn parse_arquivo_directive(&mut self, is_header: bool) -> Result<(String, Span)> {
        let kw = self.expect_keyword(Keyword::Arquivo, "arquivo")?;
        if is_header && self.has_header_arquivo {
            return Err(AlgError::parse(
                kw.span,
                "Diretiva 'arquivo' declarada em duplicidade no algoritmo.",
            ));
        }
        let has_paren = matches!(self.current().kind, TokenKind::LParen);
        if has_paren {
            self.advance();
        }
        let tok = self.current().clone();
        let path = match tok.kind {
            TokenKind::StringLit(s) => {
                self.advance();
                s
            }
            _ => {
                return Err(AlgError::parse(
                    tok.span,
                    "Esperado nome do arquivo entre aspas após 'arquivo' (ex.: arquivo \"dados.txt\").",
                ));
            }
        };
        if has_paren {
            self.expect_token(TokenKind::RParen, ")")?;
        }
        self.consume_stmt_separators()?;
        if is_header {
            self.has_header_arquivo = true;
        }
        Ok((path, kw.span))
    }

    pub(super) fn parse_var_section(
        &mut self,
        mut arquivo_slot: Option<&mut Option<(String, Span)>>,
    ) -> Result<Vec<VarDecl>> {
        let mut decls = Vec::new();
        while !self.is_decl_section_end() {
            if matches!(self.current().kind, TokenKind::Keyword(Keyword::Arquivo)) {
                if let Some(slot) = arquivo_slot.as_deref_mut() {
                    let (path, span) = self.parse_arquivo_directive(true)?;
                    *slot = Some((path, span));
                    self.skip_separators();
                    continue;
                }
                return Err(AlgError::parse(
                    self.current().span,
                    "Diretiva 'arquivo' não é permitida na seção 'var' local de sub-rotinas.",
                ));
            }

            let names = self.parse_ident_list("variável")?;
            self.expect_token(TokenKind::Colon, ":")?;
            let var_type = self.parse_var_type()?;

            for (name, span) in names {
                decls.push(VarDecl {
                    name,
                    var_type: var_type.clone(),
                    span,
                });
            }
            self.consume_decl_separators()?;
        }
        Ok(decls)
    }

    pub(super) fn parse_var_type(&mut self) -> Result<VarType> {
        let tok = self.current().clone();
        self.with_depth(tok.span, |p| p.parse_var_type_inner(tok))
    }

    fn parse_var_type_inner(&mut self, tok: crate::token::Token) -> Result<VarType> {
        let is_array = matches!(tok.kind, TokenKind::Keyword(Keyword::Vetor))
            || self.is_contextual_matriz_start();
        if is_array {
            self.advance();
            self.expect_token(TokenKind::LBracket, "[")?;
            let mut dims = Vec::new();
            loop {
                let start = self.parse_expr()?;
                self.expect_token(TokenKind::DotDot, "..")?;
                let end = self.parse_expr()?;
                dims.push(ArrayRangeExpr { start, end });
                if matches!(self.current().kind, TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect_token(TokenKind::RBracket, "]")?;
            self.expect_keyword(Keyword::De, "de")?;
            let elem_type = Box::new(self.parse_var_type()?);
            return Ok(VarType::Array { elem_type, dims });
        }

        match tok.kind {
            TokenKind::Keyword(Keyword::Registro) => {
                let start_span = self.advance().span;
                self.skip_separators();
                let mut fields = Vec::new();
                while !matches!(self.current().kind, TokenKind::Keyword(Keyword::Fimregistro)) {
                    if matches!(self.current().kind, TokenKind::Eof) {
                        return Err(AlgError::parse(
                            self.current().span,
                            format!(
                                "Esperado 'fimregistro' para fechar o 'registro' da linha {}.",
                                start_span.line
                            ),
                        ));
                    }
                    let fnames = self.parse_ident_list("campo do registro")?;
                    self.expect_token(TokenKind::Colon, ":")?;
                    let field_type = self.parse_var_type()?;
                    for (fname, fspan) in fnames {
                        fields.push(FieldDecl {
                            name: fname,
                            var_type: field_type.clone(),
                            span: fspan,
                        });
                    }
                    self.consume_decl_separators()?;
                }
                self.expect_keyword(Keyword::Fimregistro, "fimregistro")?;
                Ok(VarType::Record {
                    fields,
                    span: start_span,
                })
            }
            TokenKind::Keyword(Keyword::Inteiro) => {
                self.advance();
                Ok(VarType::Primitive(PrimitiveType::Inteiro))
            }
            TokenKind::Keyword(Keyword::Real) => {
                self.advance();
                Ok(VarType::Primitive(PrimitiveType::Real))
            }
            TokenKind::Keyword(Keyword::Caractere) => {
                self.advance();
                Ok(VarType::Primitive(PrimitiveType::Caractere))
            }
            TokenKind::Keyword(Keyword::Logico) => {
                self.advance();
                Ok(VarType::Primitive(PrimitiveType::Logico))
            }
            TokenKind::Ident(type_name) => {
                self.advance();
                Ok(VarType::Named(type_name, tok.span))
            }
            _ => Err(AlgError::parse(
                tok.span,
                format!(
                    "Tipo inválido '{}'. Tipos suportados: inteiro, real, caractere, logico, vetor, registro ou nome de tipo.",
                    tok.lexeme
                ),
            )),
        }
    }

    pub(super) fn parse_routine(&mut self, is_func: bool) -> Result<RoutineDecl> {
        let kw_tok = self.advance();
        let (name, _) = self.expect_ident(if is_func { "função" } else { "procedimento" })?;

        let mut params = Vec::new();
        if matches!(self.current().kind, TokenKind::LParen) {
            self.advance();
            if !matches!(self.current().kind, TokenKind::RParen) {
                loop {
                    let by_ref = if matches!(self.current().kind, TokenKind::Keyword(Keyword::Var))
                    {
                        self.advance();
                        true
                    } else {
                        false
                    };
                    let group_names = self.parse_ident_list("parâmetro")?;
                    self.expect_token(TokenKind::Colon, ":")?;
                    let var_type = self.parse_var_type()?;
                    for (pname, pspan) in group_names {
                        params.push(ParamDecl {
                            name: pname,
                            var_type: var_type.clone(),
                            by_ref,
                            span: pspan,
                        });
                    }
                    if matches!(self.current().kind, TokenKind::Semicolon | TokenKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            self.expect_token(TokenKind::RParen, ")")?;
        }

        let return_type = if is_func {
            self.expect_token(TokenKind::Colon, ":")?;
            Some(self.parse_var_type()?)
        } else {
            None
        };

        self.skip_separators();
        let mut locals = Vec::new();
        if matches!(self.current().kind, TokenKind::Keyword(Keyword::Var)) {
            self.advance();
            self.skip_separators();
            locals = self.parse_var_section(None)?;
        }

        self.expect_keyword(Keyword::Inicio, "inicio")?;
        self.skip_separators();

        let end_kw = if is_func {
            Keyword::Fimfuncao
        } else {
            Keyword::Fimprocedimento
        };
        let body = self.parse_stmt_list(std::slice::from_ref(&end_kw))?;
        self.expect_keyword(
            end_kw,
            if is_func {
                "fimfuncao"
            } else {
                "fimprocedimento"
            },
        )?;
        self.skip_separators();

        Ok(RoutineDecl {
            name,
            params,
            return_type,
            locals,
            body,
            span: kw_tok.span,
        })
    }
}
