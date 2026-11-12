use crate::{decode_source_bytes, run_with_options, ExecOptions};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static GOLDEN_SANDBOX_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoldenCaseResult {
    pub name: String,
    pub path: PathBuf,
    pub passed: bool,
    pub failure_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoldenSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub cases: Vec<GoldenCaseResult>,
}

impl GoldenSummary {
    pub fn is_ok(&self) -> bool {
        self.failed == 0
    }

    pub fn format_failures(&self) -> String {
        format_golden_failures(self)
    }
}

pub fn format_golden_failures(summary: &GoldenSummary) -> String {
    summary
        .cases
        .iter()
        .filter_map(|case| case.failure_message.as_deref())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct GoldenDirectives {
    rng_seed: Option<u64>,
    strict_read: bool,
}

struct TempSandboxGuard {
    path: PathBuf,
}

impl TempSandboxGuard {
    fn new() -> std::result::Result<Self, String> {
        let base_dir = std::env::temp_dir();
        let pid = std::process::id();
        for _ in 0..1000 {
            let seq = GOLDEN_SANDBOX_SEQ.fetch_add(1, Ordering::Relaxed);
            let candidate = base_dir.join(format!("algoritmoslab_golden_{pid}_{seq}"));
            match fs::create_dir(&candidate) {
                Ok(()) => return Ok(Self { path: candidate }),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    fs::create_dir_all(&base_dir).map_err(|e| {
                        format!(
                            "Não foi possível preparar diretório temporário '{}': {e}",
                            base_dir.display()
                        )
                    })?;
                    continue;
                }
                Err(err) => {
                    return Err(format!(
                        "Não foi possível criar sandbox temporário '{}': {err}",
                        candidate.display()
                    ));
                }
            }
        }
        Err("Não foi possível alocar um diretório sandbox temporário exclusivo.".to_string())
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempSandboxGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn is_golden_case_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            let lower = ext.to_ascii_lowercase();
            matches!(lower.as_str(), "alg" | "in" | "out" | "err")
        })
}

fn is_alg_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("alg"))
}

fn parse_golden_inputs(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let trimmed = text.strip_suffix('\n').unwrap_or(text);
    trimmed.split('\n').map(ToString::to_string).collect()
}

fn parse_golden_directives(source: &str) -> std::result::Result<GoldenDirectives, String> {
    let mut directives = GoldenDirectives::default();
    let mut strict_read_seen = false;

    for (idx, line) in source.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(comment_body) = trimmed.strip_prefix("//") else {
            break;
        };
        let body = comment_body.trim();
        let Some(prefix) = body.get(..7) else {
            continue;
        };
        if !prefix.eq_ignore_ascii_case("golden:") {
            continue;
        }

        let rest = body[7..].trim();
        if rest.is_empty() {
            return Err(format!("Diretiva golden vazia na linha {line_no}."));
        }

        for token in rest.split_whitespace() {
            let Some((key, val)) = token.split_once('=') else {
                return Err(format!(
                    "Diretiva golden inválida na linha {line_no}: '{token}' (esperado chave=valor)."
                ));
            };
            match key.to_ascii_lowercase().as_str() {
                "rng_seed" => {
                    if directives.rng_seed.is_some() {
                        return Err(format!(
                            "Chave de diretiva golden duplicada na linha {line_no}: 'rng_seed'."
                        ));
                    }
                    let seed = val.parse::<u64>().map_err(|_| {
                        format!(
                            "Valor inválido para 'rng_seed' na linha {line_no}: '{val}' (esperado inteiro u64)."
                        )
                    })?;
                    directives.rng_seed = Some(seed);
                }
                "strict_read" => {
                    if strict_read_seen {
                        return Err(format!(
                            "Chave de diretiva golden duplicada na linha {line_no}: 'strict_read'."
                        ));
                    }
                    let parsed = match val.to_ascii_lowercase().as_str() {
                        "true" => true,
                        "false" => false,
                        _ => {
                            return Err(format!(
                                "Valor inválido para 'strict_read' na linha {line_no}: '{val}' (esperado true|false)."
                            ));
                        }
                    };
                    directives.strict_read = parsed;
                    strict_read_seen = true;
                }
                _ => {
                    return Err(format!(
                        "Chave de diretiva golden desconhecida na linha {line_no}: '{key}'."
                    ));
                }
            }
        }
    }

    Ok(directives)
}

fn read_decoded_file(path: &Path) -> std::result::Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("Erro lendo '{}': {e}", path.display()))?;
    Ok(decode_source_bytes(&bytes).text)
}

fn copy_fixtures_to_sandbox(
    fixtures: &[PathBuf],
    sandbox_dir: &Path,
) -> std::result::Result<(), String> {
    for src in fixtures {
        let Some(file_name) = src.file_name() else {
            continue;
        };
        let dst = sandbox_dir.join(file_name);
        fs::copy(src, &dst).map_err(|e| {
            format!(
                "Erro copiando fixture '{}' para '{}': {e}",
                src.display(),
                dst.display()
            )
        })?;
    }
    Ok(())
}

fn make_case_name(alg_path: &Path) -> String {
    alg_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| alg_path.display().to_string())
}

pub fn run_golden_dir(dir: &Path) -> std::result::Result<GoldenSummary, String> {
    let read_dir = fs::read_dir(dir)
        .map_err(|e| format!("Não foi possível abrir '{}': {e}", dir.display()))?;

    let mut alg_files = Vec::new();
    let mut fixture_files = Vec::new();

    for entry_res in read_dir {
        let entry = entry_res
            .map_err(|e| format!("Erro listando entradas de '{}': {e}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| format!("Erro inspecionando '{}': {e}", path.display()))?;
        if !file_type.is_file() {
            continue;
        }
        if is_alg_file(&path) {
            alg_files.push(path);
        } else if !is_golden_case_ext(&path) {
            fixture_files.push(path);
        }
    }

    alg_files.sort();
    fixture_files.sort();

    let mut cases = Vec::with_capacity(alg_files.len());
    let mut passed = 0usize;
    let mut failed = 0usize;

    for alg_path in alg_files {
        let name = make_case_name(&alg_path);
        let in_path = alg_path.with_extension("in");
        let out_path = alg_path.with_extension("out");
        let err_path = alg_path.with_extension("err");

        let has_out = out_path.exists();
        let has_err = err_path.exists();
        if !has_out && !has_err {
            failed += 1;
            cases.push(GoldenCaseResult {
                name,
                path: alg_path.clone(),
                passed: false,
                failure_message: Some(format!(
                    "FALHOU [{}]: nenhum arquivo de referência '.out' ou '.err' encontrado.",
                    alg_path.display()
                )),
            });
            continue;
        }
        if has_out && has_err {
            failed += 1;
            cases.push(GoldenCaseResult {
                name,
                path: alg_path.clone(),
                passed: false,
                failure_message: Some(format!(
                    "FALHOU [{}]: arquivos '.out' e '.err' não podem coexistir para o mesmo caso.",
                    alg_path.display()
                )),
            });
            continue;
        }

        let alg_text = read_decoded_file(&alg_path)?;
        let directives = match parse_golden_directives(&alg_text) {
            Ok(d) => d,
            Err(msg) => {
                failed += 1;
                cases.push(GoldenCaseResult {
                    name,
                    path: alg_path.clone(),
                    passed: false,
                    failure_message: Some(format!("FALHOU [{}]: {msg}", alg_path.display())),
                });
                continue;
            }
        };

        let inputs = if in_path.exists() {
            let in_text = read_decoded_file(&in_path)?;
            parse_golden_inputs(&in_text)
        } else {
            Vec::new()
        };

        let sandbox = TempSandboxGuard::new()?;
        copy_fixtures_to_sandbox(&fixture_files, sandbox.path())?;

        let result = run_with_options(
            &alg_text,
            inputs,
            ExecOptions {
                sandbox_dir: Some(sandbox.path().to_path_buf()),
                rng_seed: directives.rng_seed,
                strict_read: directives.strict_read,
                ..Default::default()
            },
        );

        let failure_message = if has_err {
            let expected_err = read_decoded_file(&err_path)?;
            let needle = expected_err.trim();
            if needle.is_empty() {
                Some(format!(
                    "FALHOU [{}]: arquivo de referência '{}' está vazio.",
                    alg_path.display(),
                    err_path.display()
                ))
            } else {
                match result {
                    Err(e) if e.to_string().contains(needle) => None,
                    Err(e) => Some(format!(
                        "FALHOU [{}]: erro esperado contendo '{}', mas obteve '{}'",
                        alg_path.display(),
                        needle,
                        e
                    )),
                    Ok(out) => Some(format!(
                        "FALHOU [{}]: esperava erro '{}', mas concluiu com saída:\n{}",
                        alg_path.display(),
                        needle,
                        out
                    )),
                }
            }
        } else {
            let expected_out = read_decoded_file(&out_path)?;
            match result {
                Ok(actual) if actual == expected_out => None,
                Ok(actual) => Some(format!(
                    "FALHOU [{}]:\n--- esperado ---\n{}\n--- obtido ---\n{}",
                    alg_path.display(),
                    expected_out,
                    actual
                )),
                Err(e) => Some(format!(
                    "FALHOU [{}]: erro inesperado: {}",
                    alg_path.display(),
                    e
                )),
            }
        };

        if let Some(msg) = failure_message {
            failed += 1;
            cases.push(GoldenCaseResult {
                name,
                path: alg_path,
                passed: false,
                failure_message: Some(msg),
            });
        } else {
            passed += 1;
            cases.push(GoldenCaseResult {
                name,
                path: alg_path,
                passed: true,
                failure_message: None,
            });
        }
    }

    Ok(GoldenSummary {
        total: cases.len(),
        passed,
        failed,
        cases,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{encode_source, SourceEncoding};

    fn make_temp_test_dir(tag: &str) -> PathBuf {
        let seq = GOLDEN_SANDBOX_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "algoritmoslab_golden_unit_{tag}_{}_{}",
            std::process::id(),
            seq
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn runs_workspace_golden_suite_via_shared_runner() {
        let golden_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests")
            .join("golden");
        let summary = run_golden_dir(&golden_dir).unwrap();
        assert!(
            summary.total >= 40,
            "esperado ao menos 40 casos, encontrado {}",
            summary.total
        );
        assert!(
            summary.is_ok(),
            "falhas na suíte Golden:\n{}",
            summary.format_failures()
        );
    }

    #[test]
    fn reports_missing_or_conflicting_expectation_files() {
        let dir = make_temp_test_dir("missing_ref");
        fs::write(
            dir.join("01_sem_ref.alg"),
            "algoritmo \"a\"\ninicio\nfimalgoritmo\n",
        )
        .unwrap();
        fs::write(
            dir.join("02_duplo_ref.alg"),
            "algoritmo \"b\"\ninicio\nfimalgoritmo\n",
        )
        .unwrap();
        fs::write(dir.join("02_duplo_ref.out"), "").unwrap();
        fs::write(dir.join("02_duplo_ref.err"), "erro").unwrap();

        let summary = run_golden_dir(&dir).unwrap();
        assert_eq!(summary.total, 2);
        assert_eq!(summary.passed, 0);
        assert_eq!(summary.failed, 2);
        let formatted = summary.format_failures();
        assert!(formatted.contains("nenhum arquivo de referência"));
        assert!(formatted.contains("não podem coexistir"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_top_golden_directives_for_rng_seed_and_strict_read() {
        let dir = make_temp_test_dir("directives");

        fs::write(
            dir.join("01_seed.alg"),
            "// comentário inicial\n// golden: rng_seed=42 strict_read=true\nalgoritmo \"d\"\nvar\n   x: inteiro\ninicio\n   aleatorio 1, 100\n   leia(x)\n   escreval(x)\nfimalgoritmo\n",
        )
        .unwrap();
        // Executa uma vez para obter o valor determinístico com seed 42
        let deterministic_out = run_with_options(
            "algoritmo \"d\"\nvar\n   x: inteiro\ninicio\n   aleatorio 1, 100\n   leia(x)\n   escreval(x)\nfimalgoritmo\n",
            Vec::<String>::new(),
            ExecOptions {
                rng_seed: Some(42),
                strict_read: true,
                ..Default::default()
            },
        )
        .unwrap();
        fs::write(dir.join("01_seed.out"), deterministic_out).unwrap();

        // strict_read=true falha em entrada inválida
        fs::write(
            dir.join("02_strict.alg"),
            "// golden: strict_read=true\nalgoritmo \"s\"\nvar\n   x: inteiro\ninicio\n   leia(x)\nfimalgoritmo\n",
        )
        .unwrap();
        fs::write(dir.join("02_strict.in"), "abc\n").unwrap();
        fs::write(dir.join("02_strict.err"), "número inteiro válido").unwrap();

        // Diretiva malformada gera falha clara do caso
        fs::write(
            dir.join("03_bad_dir.alg"),
            "// golden: unknown_key=1\nalgoritmo \"bad\"\ninicio\nfimalgoritmo\n",
        )
        .unwrap();
        fs::write(dir.join("03_bad_dir.out"), "").unwrap();

        let summary = run_golden_dir(&dir).unwrap();
        assert_eq!(summary.total, 3);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 1);
        assert!(summary.cases[2]
            .failure_message
            .as_deref()
            .is_some_and(|m| m.contains("desconhecida")));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn preserves_empty_lines_in_inputs_and_supports_windows_1252_and_isolated_sandbox() {
        let dir = make_temp_test_dir("win1252_iso");
        fs::write(dir.join("dados.txt"), "42\n").unwrap();

        let alg_fixture = "algoritmo \"LeFixture\"\narquivo \"dados.txt\"\nvar\n   n: inteiro\ninicio\n   leia(n)\n   escreval(n)\nfimalgoritmo\n";
        fs::write(dir.join("00_fixture.alg"), alg_fixture).unwrap();
        fs::write(dir.join("00_fixture.out"), "42\n").unwrap();

        let alg_empty = "algoritmo \"Vazio\"\nvar\n   s1, s2, s3: caractere\ninicio\n   leia(s1, s2, s3)\n   escreval(\"[\", s1, \"|\", s2, \"|\", s3, \"]\")\nfimalgoritmo\n";
        fs::write(
            dir.join("01_empty_in.alg"),
            encode_source(alg_empty, SourceEncoding::Windows1252),
        )
        .unwrap();
        fs::write(
            dir.join("01_empty_in.in"),
            encode_source("Olá\n\nFim\n", SourceEncoding::Windows1252),
        )
        .unwrap();
        fs::write(
            dir.join("01_empty_in.out"),
            encode_source("[Olá||Fim]\n", SourceEncoding::Windows1252),
        )
        .unwrap();

        // Caso 2 cria um arquivo "criado.txt" dentro do sandbox; não deve vazar para `dir`!
        let alg_write = "algoritmo \"Isola\"\narquivo \"criado.txt\"\nvar\n   x: inteiro\ninicio\n   leia(x)\n   escreval(x)\nfimalgoritmo\n";
        fs::write(dir.join("02_write.alg"), alg_write).unwrap();
        fs::write(dir.join("02_write.in"), "77\n").unwrap();
        fs::write(dir.join("02_write.out"), "77\n").unwrap();

        let summary = run_golden_dir(&dir).unwrap();
        assert_eq!(summary.total, 3);
        assert!(
            summary.is_ok(),
            "falhas inesperadas:\n{}",
            summary.format_failures()
        );
        assert!(!dir.join("criado.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn supports_multi_dot_stems_for_in_out_and_err() {
        let dir = make_temp_test_dir("multi_dot");

        fs::write(
            dir.join("01.caso.teste.alg"),
            "algoritmo \"MultiDotOk\"\nvar x: inteiro\ninicio\n   leia(x)\n   escreval(x + 1)\nfimalgoritmo\n",
        )
        .unwrap();
        fs::write(dir.join("01.caso.teste.in"), "41\n").unwrap();
        fs::write(dir.join("01.caso.teste.out"), "42\n").unwrap();

        fs::write(
            dir.join("02.erro.teste.alg"),
            "algoritmo \"MultiDotErr\"\ninicio\n   escreval(1 / 0)\nfimalgoritmo\n",
        )
        .unwrap();
        fs::write(dir.join("02.erro.teste.err"), "Divisão por zero").unwrap();

        let summary = run_golden_dir(&dir).unwrap();
        assert_eq!(summary.total, 2);
        assert!(
            summary.is_ok(),
            "falhas inesperadas:\n{}",
            summary.format_failures()
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
