use encoding_rs::WINDOWS_1252;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceEncoding {
    Utf8,
    Windows1252,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecodedSource {
    pub text: String,
    pub encoding: SourceEncoding,
    pub had_crlf: bool,
}

fn normalize_newlines_to_lf(text: Cow<'_, str>) -> String {
    if !text.as_bytes().contains(&b'\r') {
        return text.into_owned();
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\r' {
            out.push_str(&text[start..i]);
            out.push('\n');
            if bytes.get(i + 1) == Some(&b'\n') {
                i += 2;
            } else {
                i += 1;
            }
            start = i;
        } else {
            i += 1;
        }
    }
    out.push_str(&text[start..]);
    out
}

fn to_crlf(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut needs_change = false;
    for (i, &b) in bytes.iter().enumerate() {
        if (b == b'\r' && bytes.get(i + 1) != Some(&b'\n'))
            || (b == b'\n' && (i == 0 || bytes[i - 1] != b'\r'))
        {
            needs_change = true;
            break;
        }
    }
    if !needs_change {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + 8);
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\r' {
            out.push_str(&text[start..i]);
            out.push_str("\r\n");
            if bytes.get(i + 1) == Some(&b'\n') {
                i += 2;
            } else {
                i += 1;
            }
            start = i;
        } else if bytes[i] == b'\n' {
            out.push_str(&text[start..i]);
            out.push_str("\r\n");
            i += 1;
            start = i;
        } else {
            i += 1;
        }
    }
    out.push_str(&text[start..]);
    Cow::Owned(out)
}

/// Decodifica bytes de um arquivo `.alg`, aceitando UTF-8 (com ou sem BOM)
/// ou aplicando fallback automático para Windows-1252 (padrão de ambientes legados em Windows).
/// Normaliza quebras de linha `\r\n` e `\r` para `\n`.
pub fn decode_source_bytes(bytes: &[u8]) -> DecodedSource {
    let had_crlf = bytes.windows(2).any(|w| w == b"\r\n");

    let (raw_text, encoding): (Cow<'_, str>, SourceEncoding) =
        if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
            match std::str::from_utf8(rest) {
                Ok(s) => (Cow::Borrowed(s), SourceEncoding::Utf8),
                Err(_) => {
                    let (cow, _, _) = WINDOWS_1252.decode(rest);
                    (cow, SourceEncoding::Windows1252)
                }
            }
        } else if let Ok(s) = std::str::from_utf8(bytes) {
            (Cow::Borrowed(s), SourceEncoding::Utf8)
        } else {
            let (cow, _, _) = WINDOWS_1252.decode(bytes);
            (cow, SourceEncoding::Windows1252)
        };

    let normalized = normalize_newlines_to_lf(raw_text);

    DecodedSource {
        text: normalized,
        encoding,
        had_crlf,
    }
}

/// Codifica o texto do editor para gravação em disco no encoding desejado.
/// Para `Windows1252`, converte quebras de linha para `\r\n` para compatibilidade máxima
/// com ambientes legados em Windows.
pub fn encode_source(text: &str, encoding: SourceEncoding) -> Vec<u8> {
    match encoding {
        SourceEncoding::Utf8 => normalize_newlines_to_lf(Cow::Borrowed(text)).into_bytes(),
        SourceEncoding::Windows1252 => {
            let crlf = to_crlf(text);
            let (cow, _, _) = WINDOWS_1252.encode(&crlf);
            cow.into_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf8_and_normalizes_crlf() {
        let input = "algoritmo \"olá\"\r\ninicio\r\nfimalgoritmo\r\n".as_bytes();
        let decoded = decode_source_bytes(input);
        assert_eq!(decoded.encoding, SourceEncoding::Utf8);
        assert!(decoded.had_crlf);
        assert_eq!(decoded.text, "algoritmo \"olá\"\ninicio\nfimalgoritmo\n");
    }

    #[test]
    fn decodes_windows_1252_fallback() {
        let (encoded, _, _) = WINDOWS_1252.encode("escreval(\"Olá, coração!\")\r\n");
        let decoded = decode_source_bytes(&encoded);
        assert_eq!(decoded.encoding, SourceEncoding::Windows1252);
        assert!(decoded.had_crlf);
        assert_eq!(decoded.text, "escreval(\"Olá, coração!\")\n");
    }

    #[test]
    fn strips_utf8_bom_when_falling_back_to_windows_1252() {
        let (encoded, _, _) = WINDOWS_1252.encode("escreval(\"Olá\")\r");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(&encoded);
        let decoded = decode_source_bytes(&bytes);
        assert_eq!(decoded.encoding, SourceEncoding::Windows1252);
        assert!(!decoded.had_crlf);
        assert_eq!(decoded.text, "escreval(\"Olá\")\n");
    }

    #[test]
    fn normalizes_mixed_and_standalone_cr_in_single_pass() {
        let decoded = decode_source_bytes(b"a\rb\r\nc\nd\r\r");
        assert!(decoded.had_crlf);
        assert_eq!(decoded.text, "a\nb\nc\nd\n\n");
    }

    #[test]
    fn roundtrips_windows_1252_with_crlf() {
        let text = "algoritmo \"Ação\"\ninicio\nfimalgoritmo\n";
        let bytes = encode_source(text, SourceEncoding::Windows1252);
        let decoded = decode_source_bytes(&bytes);
        assert_eq!(decoded.encoding, SourceEncoding::Windows1252);
        assert!(decoded.had_crlf);
        assert_eq!(decoded.text, text);
    }

    #[test]
    fn encodes_standalone_cr_to_crlf_in_windows_1252() {
        let bytes = encode_source("a\rb\r\nc\nd", SourceEncoding::Windows1252);
        assert_eq!(bytes, b"a\r\nb\r\nc\r\nd");
    }
}
