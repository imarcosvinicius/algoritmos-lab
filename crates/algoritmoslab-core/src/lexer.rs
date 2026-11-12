use crate::error::{Result, Span, AlgError};
use crate::token::{Keyword, Token, TokenKind};

pub fn normalize_keyword(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            'á' | 'à' | 'ã' | 'â' | 'Á' | 'À' | 'Ã' | 'Â' => 'a',
            'é' | 'ê' | 'è' | 'É' | 'Ê' | 'È' => 'e',
            'í' | 'ì' | 'î' | 'Í' | 'Ì' | 'Î' => 'i',
            'ó' | 'ô' | 'õ' | 'ò' | 'Ó' | 'Ô' | 'Õ' | 'Ò' => 'o',
            'ú' | 'ü' | 'ù' | 'û' | 'Ú' | 'Ü' | 'Ù' | 'Û' => 'u',
            'ç' | 'Ç' => 'c',
            other => other.to_ascii_lowercase(),
        })
        .collect()
}

fn match_keyword(word: &str) -> Option<Keyword> {
    match normalize_keyword(word).as_str() {
        "algoritmo" => Some(Keyword::Algoritmo),
        "fimalgoritmo" => Some(Keyword::Fimalgoritmo),
        "var" => Some(Keyword::Var),
        "inicio" => Some(Keyword::Inicio),
        "const" => Some(Keyword::Const),
        "tipo" => Some(Keyword::Tipo),
        "registro" => Some(Keyword::Registro),
        "fimregistro" => Some(Keyword::Fimregistro),
        "inteiro" => Some(Keyword::Inteiro),
        "real" => Some(Keyword::Real),
        "caractere" | "caracter" | "literal" => Some(Keyword::Caractere),
        "logico" => Some(Keyword::Logico),
        "vetor" => Some(Keyword::Vetor),
        "de" => Some(Keyword::De),
        "se" => Some(Keyword::Se),
        "entao" => Some(Keyword::Entao),
        "senao" => Some(Keyword::Senao),
        "fimse" => Some(Keyword::Fimse),
        "escolha" => Some(Keyword::Escolha),
        "caso" => Some(Keyword::Caso),
        "outrocaso" => Some(Keyword::Outrocaso),
        "fimescolha" => Some(Keyword::Fimescolha),
        "para" => Some(Keyword::Para),
        "ate" => Some(Keyword::Ate),
        "passo" => Some(Keyword::Passo),
        "faca" => Some(Keyword::Faca),
        "fimpara" => Some(Keyword::Fimpara),
        "enquanto" => Some(Keyword::Enquanto),
        "fimenquanto" => Some(Keyword::Fimenquanto),
        "repita" => Some(Keyword::Repita),
        "interrompa" => Some(Keyword::Interrompa),
        "procedimento" => Some(Keyword::Procedimento),
        "fimprocedimento" => Some(Keyword::Fimprocedimento),
        "funcao" => Some(Keyword::Funcao),
        "fimfuncao" => Some(Keyword::Fimfuncao),
        "retorne" => Some(Keyword::Retorne),
        "e" => Some(Keyword::E),
        "ou" => Some(Keyword::Ou),
        "xou" => Some(Keyword::Xou),
        "nao" => Some(Keyword::Nao),
        "mod" => Some(Keyword::Mod),
        "verdadeiro" => Some(Keyword::Verdadeiro),
        "falso" => Some(Keyword::Falso),
        "escreva" => Some(Keyword::Escreva),
        "escreval" => Some(Keyword::Escreval),
        "leia" => Some(Keyword::Leia),
        "limpatela" => Some(Keyword::Limpatela),
        "arquivo" => Some(Keyword::Arquivo),
        "aleatorio" => Some(Keyword::Aleatorio),
        "timer" => Some(Keyword::Timer),
        "cronometro" => Some(Keyword::Cronometro),
        "pausa" => Some(Keyword::Pausa),
        "debug" => Some(Keyword::Debug),
        "eco" => Some(Keyword::Eco),
        "mudacor" => Some(Keyword::Mudacor),
        "dos" => Some(Keyword::Dos),
        _ => None,
    }
}

pub fn tokenize(source: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;

    let peek = |idx: usize| -> Option<char> { chars.get(idx).copied() };

    while i < chars.len() {
        let ch = chars[i];

        if ch == ' ' || ch == '\t' {
            i += 1;
            col += 1;
            continue;
        }

        if ch == '\n' {
            tokens.push(Token {
                kind: TokenKind::Newline,
                lexeme: "\n".to_string(),
                span: Span::new(line, col, 1),
            });
            i += 1;
            line += 1;
            col = 1;
            continue;
        }

        if ch == '\r' {
            if peek(i + 1) == Some('\n') {
                i += 1;
                continue;
            }
            tokens.push(Token {
                kind: TokenKind::Newline,
                lexeme: "\n".to_string(),
                span: Span::new(line, col, 1),
            });
            i += 1;
            line += 1;
            col = 1;
            continue;
        }

        // Comentário de linha //
        if ch == '/' && peek(i + 1) == Some('/') {
            i += 2;
            col += 2;
            while i < chars.len() && chars[i] != '\n' && chars[i] != '\r' {
                i += 1;
                col += 1;
            }
            continue;
        }

        // String literal "..."
        if ch == '"' {
            let start_col = col;
            i += 1;
            col += 1;
            let mut value = String::new();
            let mut closed = false;
            while i < chars.len() {
                let c = chars[i];
                if c == '\n' || c == '\r' {
                    break;
                }
                if c == '"' {
                    i += 1;
                    col += 1;
                    closed = true;
                    break;
                }
                value.push(c);
                i += 1;
                col += 1;
            }
            if !closed {
                return Err(AlgError::lex(
                    Span::new(line, start_col, col - start_col),
                    "Texto (string) não fechado com aspas duplas (\") antes do fim da linha.",
                ));
            }
            let len = value.chars().count() + 2;
            tokens.push(Token {
                kind: TokenKind::StringLit(value.clone()),
                lexeme: format!("\"{value}\""),
                span: Span::new(line, start_col, len),
            });
            continue;
        }

        // Número inteiro ou real
        if ch.is_ascii_digit() {
            let start = i;
            let start_col = col;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
                col += 1;
            }
            let mut is_real = false;
            if peek(i) == Some('.') && peek(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                is_real = true;
                i += 1;
                col += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                    col += 1;
                }
            }
            let lexeme: String = chars[start..i].iter().collect();
            let span = Span::new(line, start_col, lexeme.len());
            if is_real {
                let num: f64 = lexeme.parse().map_err(|_| {
                    AlgError::lex(span, format!("Número real inválido: '{lexeme}'."))
                })?;
                if !num.is_finite() {
                    return Err(AlgError::lex(
                        span,
                        format!("Número real fora do limite suportado: '{lexeme}'."),
                    ));
                }
                tokens.push(Token {
                    kind: TokenKind::RealLit(num),
                    lexeme,
                    span,
                });
            } else {
                let num: i64 = lexeme.parse().map_err(|_| {
                    AlgError::lex(
                        span,
                        format!("Número inteiro fora do limite suportado: '{lexeme}'."),
                    )
                })?;
                tokens.push(Token {
                    kind: TokenKind::IntLit(num),
                    lexeme,
                    span,
                });
            }
            continue;
        }

        // Identificador ou palavra-chave
        if ch.is_alphabetic() || ch == '_' {
            let start = i;
            let start_col = col;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
                col += 1;
            }
            let lexeme: String = chars[start..i].iter().collect();
            let span = Span::new(line, start_col, lexeme.chars().count());
            let kind = match match_keyword(&lexeme) {
                Some(kw) => TokenKind::Keyword(kw),
                None => TokenKind::Ident(lexeme.clone()),
            };
            tokens.push(Token { kind, lexeme, span });
            continue;
        }

        // Operadores de dois caracteres
        let two = (ch, peek(i + 1));
        if let Some((kind, lex)) = match two {
            ('<', Some('-')) => Some((TokenKind::Assign, "<-")),
            ('<', Some('>')) => Some((TokenKind::Neq, "<>")),
            ('<', Some('=')) => Some((TokenKind::Le, "<=")),
            ('>', Some('=')) => Some((TokenKind::Ge, ">=")),
            ('.', Some('.')) => Some((TokenKind::DotDot, "..")),
            _ => None,
        } {
            tokens.push(Token {
                kind,
                lexeme: lex.to_string(),
                span: Span::new(line, col, 2),
            });
            i += 2;
            col += 2;
            continue;
        }

        // Operadores e pontuação de um caractere
        let single = match ch {
            '=' => Some(TokenKind::Eq),
            '<' => Some(TokenKind::Lt),
            '>' => Some(TokenKind::Gt),
            '+' => Some(TokenKind::Plus),
            '-' => Some(TokenKind::Minus),
            '*' => Some(TokenKind::Star),
            '/' => Some(TokenKind::Slash),
            '\\' => Some(TokenKind::Backslash),
            '^' => Some(TokenKind::Caret),
            '%' => Some(TokenKind::Percent),
            '.' => Some(TokenKind::Dot),
            '(' => Some(TokenKind::LParen),
            ')' => Some(TokenKind::RParen),
            '[' => Some(TokenKind::LBracket),
            ']' => Some(TokenKind::RBracket),
            ',' => Some(TokenKind::Comma),
            ':' => Some(TokenKind::Colon),
            ';' => Some(TokenKind::Semicolon),
            _ => None,
        };

        if let Some(kind) = single {
            tokens.push(Token {
                kind,
                lexeme: ch.to_string(),
                span: Span::new(line, col, 1),
            });
            i += 1;
            col += 1;
            continue;
        }

        return Err(AlgError::lex(
            Span::new(line, col, 1),
            format!("Caractere inesperado: '{ch}'."),
        ));
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        lexeme: String::new(),
        span: Span::new(line, col, 1),
    });

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_case_insensitive_and_accented_keywords() {
        let toks = tokenize("INÍCIO se ENTÃO senão NÃO fimse").unwrap();
        assert_eq!(toks[0].kind, TokenKind::Keyword(Keyword::Inicio));
        assert_eq!(toks[1].kind, TokenKind::Keyword(Keyword::Se));
        assert_eq!(toks[2].kind, TokenKind::Keyword(Keyword::Entao));
        assert_eq!(toks[3].kind, TokenKind::Keyword(Keyword::Senao));
        assert_eq!(toks[4].kind, TokenKind::Keyword(Keyword::Nao));
        assert_eq!(toks[5].kind, TokenKind::Keyword(Keyword::Fimse));
    }

    #[test]
    fn distinguishes_range_dotdot_from_real_literal() {
        let toks = tokenize("vetor[1..10] 2.75 turma[i].nota").unwrap();
        assert_eq!(toks[0].kind, TokenKind::Keyword(Keyword::Vetor));
        assert_eq!(toks[1].kind, TokenKind::LBracket);
        assert_eq!(toks[2].kind, TokenKind::IntLit(1));
        assert_eq!(toks[3].kind, TokenKind::DotDot);
        assert_eq!(toks[4].kind, TokenKind::IntLit(10));
        assert_eq!(toks[5].kind, TokenKind::RBracket);
        assert_eq!(toks[6].kind, TokenKind::RealLit(2.75));
        assert_eq!(toks[7].kind, TokenKind::Ident("turma".to_string()));
        assert_eq!(toks[8].kind, TokenKind::LBracket);
        assert_eq!(toks[9].kind, TokenKind::Ident("i".to_string()));
        assert_eq!(toks[10].kind, TokenKind::RBracket);
        assert_eq!(toks[11].kind, TokenKind::Dot);
        assert_eq!(toks[12].kind, TokenKind::Ident("nota".to_string()));
    }

    #[test]
    fn recognizes_v02_accented_keywords() {
        let toks = tokenize("ALEATÓRIO cronômetro mudacor tipo registro fimregistro").unwrap();
        assert_eq!(toks[0].kind, TokenKind::Keyword(Keyword::Aleatorio));
        assert_eq!(toks[1].kind, TokenKind::Keyword(Keyword::Cronometro));
        assert_eq!(toks[2].kind, TokenKind::Keyword(Keyword::Mudacor));
        assert_eq!(toks[3].kind, TokenKind::Keyword(Keyword::Tipo));
        assert_eq!(toks[4].kind, TokenKind::Keyword(Keyword::Registro));
        assert_eq!(toks[5].kind, TokenKind::Keyword(Keyword::Fimregistro));
    }

    #[test]
    fn treats_matriz_and_div_as_identifiers() {
        let toks = tokenize("matriz div").unwrap();
        assert_eq!(toks[0].kind, TokenKind::Ident("matriz".to_string()));
        assert_eq!(toks[1].kind, TokenKind::Ident("div".to_string()));
    }

    #[test]
    fn tracks_line_numbers_across_standalone_cr() {
        let toks = tokenize("a\rb\r\nc//cmt\rd").unwrap();
        assert_eq!(toks[0].span.line, 1);
        assert_eq!(toks[1].kind, TokenKind::Newline);
        assert_eq!(toks[2].span.line, 2);
        assert_eq!(toks[3].kind, TokenKind::Newline);
        assert_eq!(toks[4].span.line, 3);
        assert_eq!(toks[5].kind, TokenKind::Newline);
        assert_eq!(toks[6].span.line, 4);
    }
}
