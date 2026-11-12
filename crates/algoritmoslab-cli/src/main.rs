use std::env;
use std::fs::{self, File};
use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use algoritmoslab_core::golden::{format_golden_failures, run_golden_dir};
use algoritmoslab_core::{
    decode_source_bytes, run_source, ConsoleColorTarget, ExecOptions, IoHandler, Result,
    RunController, Span, AlgError,
};

const MAX_ALG_FILE_BYTES: u64 = 4 * 1024 * 1024;
const CLI_STACK_SIZE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
enum CliCommand {
    Help,
    Version,
    Golden(PathBuf),
    RunFile(PathBuf),
}

struct StdIo {
    stdin: io::Stdin,
    echo_non_tty: bool,
}

impl StdIo {
    fn new() -> Self {
        let stdin = io::stdin();
        let echo_non_tty = !stdin.is_terminal();
        Self {
            stdin,
            echo_non_tty,
        }
    }
}

fn read_line_from_buf<R: BufRead>(
    reader: &mut R,
    var_name: &str,
    expected_type: &str,
    line_no: usize,
) -> Result<String> {
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).map_err(|e| {
        AlgError::runtime(
            (line_no > 0).then(|| Span::new(line_no, 1, 1)),
            format!("Falha ao ler entrada para '{var_name}' ({expected_type}): {e}"),
        )
    })?;
    if bytes_read == 0 {
        return Err(AlgError::runtime(
            (line_no > 0).then(|| Span::new(line_no, 1, 1)),
            format!("Fim da entrada padrão (EOF) ao tentar ler '{var_name}'."),
        ));
    }
    Ok(line.trim_end_matches(&['\r', '\n'][..]).to_string())
}

impl IoHandler for StdIo {
    fn write(&mut self, text: &str) {
        print!("{text}");
        let _ = io::stdout().flush();
    }

    fn clear(&mut self) {
        print!("\x1B[2J\x1B[H");
        let _ = io::stdout().flush();
    }

    fn write_error(&mut self, text: &str) {
        eprintln!("[Erro] {text}");
    }

    fn read_input(&mut self, var_name: &str, expected_type: &str, line: usize) -> Result<String> {
        print!("leia({var_name}: {expected_type}) > ");
        let _ = io::stdout().flush();
        let mut lock = self.stdin.lock();
        read_line_from_buf(&mut lock, var_name, expected_type, line)
    }

    fn echo_input(&mut self, text: &str) {
        // Em terminal interativo o próprio TTY já exibe a digitação do usuário;
        // quando redirecionado via pipe/arquivo, ecoamos explicitamente para refletir `eco on/off`.
        if self.echo_non_tty {
            println!("{text}");
            let _ = io::stdout().flush();
        }
    }

    fn set_console_color(
        &mut self,
        target: ConsoleColorTarget,
        _color_hex: &str,
        color_name: &str,
    ) {
        let _ = (target, color_name);
    }
}

fn parse_cli_args(args: &[String]) -> std::result::Result<CliCommand, String> {
    if args.is_empty() {
        return Ok(CliCommand::Help);
    }

    let first = &args[0];
    match first.as_str() {
        "-h" | "--help" => {
            if args.len() > 1 {
                return Err(format!(
                    "Argumento inesperado '{}'. Uso: algoritmoslab-cli --help",
                    args[1]
                ));
            }
            Ok(CliCommand::Help)
        }
        "-V" | "--version" => {
            if args.len() > 1 {
                return Err(format!(
                    "Argumento inesperado '{}'. Uso: algoritmoslab-cli --version",
                    args[1]
                ));
            }
            Ok(CliCommand::Version)
        }
        "--golden" => match args.len() {
            1 => Ok(CliCommand::Golden(PathBuf::from("tests/golden"))),
            2 => {
                let dir = &args[1];
                if dir.starts_with('-') {
                    return Err(format!(
                        "Argumento inválido após '--golden': '{dir}'. Esperado caminho de diretório."
                    ));
                }
                Ok(CliCommand::Golden(PathBuf::from(dir)))
            }
            _ => Err(format!(
                "Argumentos inesperados após '--golden': '{}'. Uso: algoritmoslab-cli --golden [diretorio]",
                args[2..].join(" ")
            )),
        },
        arg if arg.starts_with('-') => Err(format!(
            "Opção desconhecida: '{arg}'. Use '--help' para ver as opções disponíveis."
        )),
        file_arg => {
            if args.len() > 1 {
                return Err(format!(
                    "Argumento inesperado '{}'. Uso: algoritmoslab-cli <arquivo.alg>",
                    args[1]
                ));
            }
            Ok(CliCommand::RunFile(PathBuf::from(file_arg)))
        }
    }
}

fn read_alg_file_bounded(path: &Path) -> std::result::Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path)
        .map_err(|e| format!("Erro ao abrir '{}': {e}", path.display()))?;
    if meta.file_type().is_symlink() {
        return Err(format!(
            "Erro ao abrir '{}': links simbólicos não são permitidos.",
            path.display()
        ));
    }
    if meta.is_dir() {
        return Err(format!(
            "Erro ao abrir '{}': esperado arquivo '.alg', mas é um diretório.",
            path.display()
        ));
    }
    if !meta.file_type().is_file() {
        return Err(format!(
            "Erro ao abrir '{}': não é um arquivo regular.",
            path.display()
        ));
    }
    if meta.len() > MAX_ALG_FILE_BYTES {
        return Err(format!(
            "Erro ao abrir '{}': arquivo excede o limite máximo de {} bytes.",
            path.display(),
            MAX_ALG_FILE_BYTES
        ));
    }

    let file = File::open(path)
        .map_err(|e| format!("Erro ao abrir '{}': {e}", path.display()))?;
    let opened_meta = file
        .metadata()
        .map_err(|e| format!("Erro ao inspecionar '{}': {e}", path.display()))?;
    if !opened_meta.file_type().is_file() {
        return Err(format!(
            "Erro ao abrir '{}': o alvo aberto não é um arquivo regular.",
            path.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened_meta.dev() != meta.dev() || opened_meta.ino() != meta.ino() {
            return Err(format!(
                "Erro ao abrir '{}': o arquivo foi alterado durante a abertura.",
                path.display()
            ));
        }
    }

    let cap = (meta.len() as usize).min(MAX_ALG_FILE_BYTES as usize);
    let mut bytes = Vec::with_capacity(cap);
    file.take(MAX_ALG_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Erro ao ler '{}': {e}", path.display()))?;
    if bytes.len() as u64 > MAX_ALG_FILE_BYTES {
        return Err(format!(
            "Erro ao abrir '{}': arquivo excede o limite máximo de {} bytes.",
            path.display(),
            MAX_ALG_FILE_BYTES
        ));
    }
    Ok(bytes)
}

fn print_help() {
    println!(
        "AlgoritmosLab CLI (algoritmoslab-core v{})",
        algoritmoslab_core::crate_version()
    );
    println!("Uso:");
    println!("  algoritmoslab-cli <arquivo.alg>");
    println!("  algoritmoslab-cli --golden [diretorio]");
    println!("  algoritmoslab-cli --help | -h");
    println!("  algoritmoslab-cli --version | -V");
}

fn run_cli(args: &[String]) -> i32 {
    let cmd = match parse_cli_args(args) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{msg}");
            return 1;
        }
    };

    match cmd {
        CliCommand::Help => {
            print_help();
            0
        }
        CliCommand::Version => {
            println!(
                "algoritmoslab-cli v{} (algoritmoslab-core v{})",
                env!("CARGO_PKG_VERSION"),
                algoritmoslab_core::crate_version()
            );
            0
        }
        CliCommand::Golden(dir) => match run_golden_dir(&dir) {
            Ok(summary) => {
                if !summary.is_ok() {
                    eprintln!("{}", format_golden_failures(&summary));
                }
                println!(
                    "Suíte Golden: {} aprovados, {} falharam.",
                    summary.passed, summary.failed
                );
                if summary.failed > 0 {
                    1
                } else {
                    0
                }
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        },
        CliCommand::RunFile(path) => {
            let bytes = match read_alg_file_bounded(&path) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("{e}");
                    return 1;
                }
            };
            let decoded = decode_source_bytes(&bytes);
            let sandbox_dir = path
                .canonicalize()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                .or_else(|| env::current_dir().ok());
            let mut io = StdIo::new();
            let mut debug = RunController::noop();
            if let Err(e) = run_source(
                &decoded.text,
                &mut io,
                &mut debug,
                ExecOptions {
                    sandbox_dir,
                    ..Default::default()
                },
            ) {
                eprintln!("\nErro de execução: {e}");
                return 1;
            }
            0
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    // No Windows a thread `main` tem pilha padrão de apenas 1 MiB; executamos a CLI em uma
    // thread dedicada de 8 MiB (mesmo patamar da VM no host desktop e do Linux).
    let code = match std::thread::Builder::new()
        .name("algoritmoslab-cli".to_string())
        .stack_size(CLI_STACK_SIZE_BYTES)
        .spawn(move || run_cli(&args))
    {
        Ok(handle) => match handle.join() {
            Ok(code) => code,
            Err(_) => {
                eprintln!("Erro interno inesperado durante a execução da CLI.");
                1
            }
        },
        Err(err) => {
            eprintln!("Falha ao iniciar thread da CLI: {err}");
            1
        }
    };
    if code != 0 {
        process::exit(code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicU64, Ordering};

    static CLI_TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(tag: &str) -> Self {
            let base = env::temp_dir();
            let pid = process::id();
            for _ in 0..1000 {
                let seq = CLI_TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
                let candidate = base.join(format!("algoritmoslab_cli_test_{tag}_{pid}_{seq}"));
                if fs::create_dir(&candidate).is_ok() {
                    return Self { path: candidate };
                }
            }
            panic!("Não foi possível criar diretório temporário para teste da CLI");
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|v| (*v).to_string()).collect()
    }

    #[test]
    fn parses_help_and_version_flags() {
        assert_eq!(parse_cli_args(&[]).unwrap(), CliCommand::Help);
        assert_eq!(parse_cli_args(&s(&["-h"])).unwrap(), CliCommand::Help);
        assert_eq!(parse_cli_args(&s(&["--help"])).unwrap(), CliCommand::Help);
        assert_eq!(parse_cli_args(&s(&["-V"])).unwrap(), CliCommand::Version);
        assert_eq!(
            parse_cli_args(&s(&["--version"])).unwrap(),
            CliCommand::Version
        );

        assert_eq!(run_cli(&s(&["--help"])), 0);
        assert_eq!(run_cli(&s(&["--version"])), 0);
    }

    #[test]
    fn rejects_unknown_flags_and_extra_arguments() {
        let err_flag = parse_cli_args(&s(&["--foo"])).unwrap_err();
        assert!(err_flag.contains("Opção desconhecida"));

        let err_short = parse_cli_args(&s(&["-x"])).unwrap_err();
        assert!(err_short.contains("Opção desconhecida"));

        let err_help_extra = parse_cli_args(&s(&["--help", "extra"])).unwrap_err();
        assert!(err_help_extra.contains("Argumento inesperado"));

        let err_ver_extra = parse_cli_args(&s(&["-V", "extra"])).unwrap_err();
        assert!(err_ver_extra.contains("Argumento inesperado"));

        let err_file_extra = parse_cli_args(&s(&["a.alg", "b.alg"])).unwrap_err();
        assert!(err_file_extra.contains("Argumento inesperado"));

        assert_eq!(run_cli(&s(&["--foo"])), 1);
    }

    #[test]
    fn parses_golden_flag_and_rejects_extra_or_flag_args() {
        assert_eq!(
            parse_cli_args(&s(&["--golden"])).unwrap(),
            CliCommand::Golden(PathBuf::from("tests/golden"))
        );
        assert_eq!(
            parse_cli_args(&s(&["--golden", "meus_testes"])).unwrap(),
            CliCommand::Golden(PathBuf::from("meus_testes"))
        );

        let err_extra = parse_cli_args(&s(&["--golden", "dir1", "dir2"])).unwrap_err();
        assert!(err_extra.contains("Argumentos inesperados"));

        let err_flag_after = parse_cli_args(&s(&["--golden", "--help"])).unwrap_err();
        assert!(err_flag_after.contains("Argumento inválido"));
    }

    #[test]
    fn parses_regular_alg_file_path() {
        assert_eq!(
            parse_cli_args(&s(&["exemplos/ola.alg"])).unwrap(),
            CliCommand::RunFile(PathBuf::from("exemplos/ola.alg"))
        );
    }

    #[test]
    fn read_line_from_buf_handles_lines_and_returns_error_on_eof() {
        let mut cursor = Cursor::new(b"42\r\n\noi\n".as_slice());
        assert_eq!(
            read_line_from_buf(&mut cursor, "n", "inteiro", 5).unwrap(),
            "42"
        );
        assert_eq!(
            read_line_from_buf(&mut cursor, "vazio", "caractere", 6).unwrap(),
            ""
        );
        assert_eq!(
            read_line_from_buf(&mut cursor, "s", "caractere", 7).unwrap(),
            "oi"
        );

        // Próxima leitura está em EOF (0 bytes): deve falhar em vez de retornar ""
        let eof_err = read_line_from_buf(&mut cursor, "fim", "inteiro", 8).unwrap_err();
        assert!(eof_err.message.contains("EOF"));
        assert_eq!(eof_err.span, Some(Span::new(8, 1, 1)));
    }

    #[test]
    fn vm_terminates_cleanly_on_stdin_eof_without_infinite_loop() {
        struct BufIo<'a> {
            reader: Cursor<&'a [u8]>,
            out: String,
            errors: Vec<String>,
        }
        impl IoHandler for BufIo<'_> {
            fn write(&mut self, text: &str) {
                self.out.push_str(text);
            }
            fn clear(&mut self) {
                self.out.clear();
            }
            fn write_error(&mut self, text: &str) {
                self.errors.push(text.to_string());
            }
            fn read_input(
                &mut self,
                var_name: &str,
                expected_type: &str,
                line: usize,
            ) -> Result<String> {
                read_line_from_buf(&mut self.reader, var_name, expected_type, line)
            }
        }

        let src = "algoritmo \"Eof\"\nvar\n   x: inteiro\ninicio\n   leia(x)\nfimalgoritmo\n";
        let mut io = BufIo {
            reader: Cursor::new(b""),
            out: String::new(),
            errors: Vec::new(),
        };
        let mut dbg = RunController::noop();
        let err = run_source(src, &mut io, &mut dbg, ExecOptions::default()).unwrap_err();
        assert!(err.message.contains("EOF"));
        assert!(io.errors.is_empty());
    }

    #[test]
    fn read_alg_file_bounded_rejects_oversized_directories_and_symlinks() {
        let dir = TempDirGuard::new("file_bounds");

        // Diretório rejeitado
        let err_dir = read_alg_file_bounded(dir.path()).unwrap_err();
        assert!(err_dir.contains("diretório"));

        // Arquivo regular válido lido corretamente
        let valid_path = dir.path().join("ok.alg");
        fs::write(&valid_path, "algoritmo \"Ok\"\ninicio\nfimalgoritmo\n").unwrap();
        let bytes = read_alg_file_bounded(&valid_path).unwrap();
        assert!(bytes.starts_with(b"algoritmo"));

        // Arquivo > 4 MiB rejeitado antes de carregar em memória
        let big_path = dir.path().join("grande.alg");
        let big_file = File::create(&big_path).unwrap();
        big_file.set_len(MAX_ALG_FILE_BYTES + 1).unwrap();
        let err_big = read_alg_file_bounded(&big_path).unwrap_err();
        assert!(err_big.contains("limite máximo"));

        // Link simbólico e arquivo especial (FIFO) rejeitados
        #[cfg(unix)]
        {
            let link_path = dir.path().join("atalho.alg");
            std::os::unix::fs::symlink(&valid_path, &link_path).unwrap();
            let err_link = read_alg_file_bounded(&link_path).unwrap_err();
            assert!(err_link.contains("links simbólicos"));

            let fifo_path = dir.path().join("pipe.alg");
            if let Ok(status) = process::Command::new("mkfifo").arg(&fifo_path).status() {
                if status.success() {
                    let err_fifo = read_alg_file_bounded(&fifo_path).unwrap_err();
                    assert!(err_fifo.contains("arquivo regular"));
                }
            }
        }
    }

    #[test]
    fn run_cli_golden_returns_exit_code_based_on_summary() {
        let dir = TempDirGuard::new("golden_exit");
        fs::write(
            dir.path().join("01_ok.alg"),
            "algoritmo \"Ok\"\ninicio\n   escreval(1 + 1)\nfimalgoritmo\n",
        )
        .unwrap();
        fs::write(dir.path().join("01_ok.out"), "2\n").unwrap();
        assert_eq!(
            run_cli(&[
                "--golden".to_string(),
                dir.path().to_string_lossy().into_owned()
            ]),
            0
        );

        fs::write(dir.path().join("01_ok.out"), "999\n").unwrap();
        assert_eq!(
            run_cli(&[
                "--golden".to_string(),
                dir.path().to_string_lossy().into_owned()
            ]),
            1
        );
    }
}
