use std::collections::VecDeque;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use algoritmoslab_core::ast::{BinaryOp, Expr};
use algoritmoslab_core::parser::parse_program;
use algoritmoslab_core::{
    compile, decode_source_bytes, encode_source, run_source, run_with_inputs, run_with_options,
    ConsoleColorTarget, DebugController, DebugSnapshot, ErrorKind, ExecOptions, Interpreter,
    IoHandler, MemoryIo, Result, RunController, SourceEncoding, Span, VarSnapshot,
};

static TEST_SANDBOX_SEQ: AtomicU64 = AtomicU64::new(0);

struct TempSandboxDir {
    path: PathBuf,
}

impl TempSandboxDir {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir();
        let pid = std::process::id();
        for _ in 0..1000 {
            let seq = TEST_SANDBOX_SEQ.fetch_add(1, Ordering::Relaxed);
            let candidate = base.join(format!("algoritmoslab_it_{tag}_{pid}_{seq}"));
            match fs::create_dir(&candidate) {
                Ok(()) => return Self { path: candidate },
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!(
                    "Falha ao criar diretório temporário '{}': {e}",
                    candidate.display()
                ),
            }
        }
        panic!("Não foi possível alocar diretório temporário exclusivo para '{tag}'");
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempSandboxDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct RecordingDebugger {
    pub lines: Vec<usize>,
    pub paused_directive_lines: Vec<usize>,
    pub snapshots: Vec<(usize, Vec<VarSnapshot>)>,
    pub call_stacks: Vec<Vec<String>>,
    pub cancel_after: Option<usize>,
    pub cancel_flag: Arc<AtomicBool>,
}

impl RecordingDebugger {
    fn new(cancel_after: Option<usize>) -> Self {
        Self {
            lines: Vec::new(),
            paused_directive_lines: Vec::new(),
            snapshots: Vec::new(),
            call_stacks: Vec::new(),
            cancel_after,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl DebugController for RecordingDebugger {
    fn check_cancel(&self) -> Result<()> {
        if self.cancel_flag.load(Ordering::Relaxed) {
            Err(algoritmoslab_core::AlgError::interrupted())
        } else {
            Ok(())
        }
    }

    fn before_statement(
        &mut self,
        line: usize,
        _call_depth: usize,
        snapshot: &mut dyn FnMut() -> DebugSnapshot,
    ) -> Result<()> {
        self.check_cancel()?;
        let snap = snapshot();
        self.lines.push(line);
        self.snapshots.push((line, snap.variables));
        self.call_stacks.push(snap.call_stack);
        if let Some(limit) = self.cancel_after {
            if self.lines.len() >= limit {
                self.cancel_flag.store(true, Ordering::Relaxed);
            }
        }
        Ok(())
    }

    fn pause_directive(
        &mut self,
        line: usize,
        _call_depth: usize,
        _snapshot: &mut dyn FnMut() -> DebugSnapshot,
    ) -> Result<()> {
        self.check_cancel()?;
        self.paused_directive_lines.push(line);
        Ok(())
    }
}

#[test]
fn runs_all_golden_alg_files() {
    let golden_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("golden");

    let summary = algoritmoslab_core::golden::run_golden_dir(&golden_dir)
        .expect("suíte Golden deve executar sem erro de I/O");
    assert!(
        summary.total >= 50,
        "esperado ao menos 50 testes Golden, encontrado {}",
        summary.total
    );
    assert!(
        summary.is_ok(),
        "falhas na suíte Golden:\n{}",
        summary.format_failures()
    );
}

#[test]
fn debug_traces_lines_and_variable_snapshots() {
    let src = r#"algoritmo "depura"
var
   x, soma: inteiro
inicio
   x <- 10
   soma <- x + 5
   escreval(soma)
fimalgoritmo
"#;
    let prog = compile(src).unwrap();
    let mut io = MemoryIo::new(Vec::<String>::new());
    let mut dbg = RecordingDebugger::new(None);
    let mut vm = Interpreter::new(&prog, &mut io, &mut dbg, ExecOptions::default()).unwrap();
    vm.run().unwrap();

    assert_eq!(io.output, "15\n");
    assert_eq!(dbg.lines, vec![5, 6, 7]);
    // Na linha 7 (antes do escreval), x=10 e soma=15 já devem estar visíveis no snapshot
    let (_, vars_line_7) = &dbg.snapshots[2];
    assert_eq!(vars_line_7[0].name, "x");
    assert_eq!(vars_line_7[0].value_text, "10");
    assert_eq!(vars_line_7[1].name, "soma");
    assert_eq!(vars_line_7[1].value_text, "15");
    assert_eq!(dbg.call_stacks[2], vec!["algoritmo \"depura\""]);
}

#[test]
fn captures_array_items_and_call_stack_in_debug_snapshot() {
    let src = r#"algoritmo "snapshot_matriz"
var
   m: matriz[1..2, -1..0] de inteiro

procedimento preencher(valor: inteiro)
inicio
   m[2, 0] <- valor
fimprocedimento

inicio
   preencher(42)
fimalgoritmo
"#;
    let prog = compile(src).unwrap();
    let mut io = MemoryIo::new(Vec::<String>::new());
    let mut dbg = RecordingDebugger::new(None);
    let mut vm = Interpreter::new(&prog, &mut io, &mut dbg, ExecOptions::default()).unwrap();
    vm.run().unwrap();

    let snap = vm.debug_snapshot();
    let m_var = snap.variables.iter().find(|v| v.name == "m").unwrap();
    assert_eq!(m_var.total_items, Some(4));
    assert_eq!(m_var.items.len(), 4);
    assert_eq!(m_var.items[0].index_label, "[1, -1]");
    assert_eq!(m_var.items[3].index_label, "[2, 0]");
    assert_eq!(m_var.items[3].value_text, "42");

    // Linha 7 é dentro de `preencher`
    let idx_in_proc = dbg.lines.iter().position(|&l| l == 7).unwrap();
    assert_eq!(
        dbg.call_stacks[idx_in_proc],
        vec!["algoritmo \"snapshot_matriz\"", "preencher"]
    );
}

#[test]
fn cancels_infinite_loop_cleanly() {
    let src = r#"algoritmo "infinito"
inicio
   enquanto verdadeiro faca
   fimenquanto
fimalgoritmo
"#;
    let mut io = MemoryIo::new(Vec::<String>::new());
    let mut dbg = RecordingDebugger::new(Some(5));
    let err = run_source(src, &mut io, &mut dbg, ExecOptions::default()).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Interrompido);
}

#[test]
fn reprompts_on_invalid_read_unless_strict() {
    struct ReReadIo {
        inputs: VecDeque<String>,
        errors: Vec<String>,
        out: String,
    }
    impl IoHandler for ReReadIo {
        fn write(&mut self, text: &str) {
            self.out.push_str(text);
        }
        fn clear(&mut self) {
            self.out.clear();
        }
        fn write_error(&mut self, text: &str) {
            self.errors.push(text.to_string());
        }
        fn read_input(&mut self, _v: &str, _t: &str, _l: usize) -> Result<String> {
            Ok(self.inputs.pop_front().unwrap())
        }
    }

    let src = r#"algoritmo "leia_seguro"
var
   idade: inteiro
inicio
   leia(idade)
   escreva(idade)
fimalgoritmo
"#;
    let mut io = ReReadIo {
        inputs: VecDeque::from(vec!["abc".to_string(), "21".to_string()]),
        errors: Vec::new(),
        out: String::new(),
    };
    let mut dbg = RecordingDebugger::new(None);
    run_source(src, &mut io, &mut dbg, ExecOptions::default()).unwrap();
    assert_eq!(io.errors.len(), 1);
    assert_eq!(io.out, "21");

    let mut strict_io = ReReadIo {
        inputs: VecDeque::from(vec!["abc".to_string()]),
        errors: Vec::new(),
        out: String::new(),
    };
    let mut strict_dbg = RecordingDebugger::new(None);
    let err = run_source(
        src,
        &mut strict_io,
        &mut strict_dbg,
        ExecOptions {
            strict_read: true,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Execucao);
}

#[test]
fn runs_windows_1252_encoded_program() {
    let src = "algoritmo \"Ação\"\nvar\n   msg: caractere\ninicio\n   msg <- \"Olá, coração!\"\n   escreval(msg)\nfimalgoritmo\n";
    let win_bytes = encode_source(src, SourceEncoding::Windows1252);
    let decoded = decode_source_bytes(&win_bytes);
    assert_eq!(decoded.encoding, SourceEncoding::Windows1252);
    let out = run_with_inputs(&decoded.text, Vec::<String>::new()).unwrap();
    assert_eq!(out, "Olá, coração!\n");
}

#[test]
fn captures_record_and_array_of_records_snapshots() {
    let src = r#"algoritmo "snap_reg"
tipo
   TAluno = registro
      nome: caractere
      nota: real
   fimregistro
var
   unico: TAluno
   turma: vetor[1..2] de TAluno
inicio
   unico.nome <- "Clara"
   unico.nota <- 9.5
   turma[1].nome <- "Pedro"
   turma[1].nota <- 7
   turma[2].nome <- "Sofia"
   turma[2].nota <- 10
fimalgoritmo
"#;
    let prog = compile(src).unwrap();
    let mut io = MemoryIo::new(Vec::<String>::new());
    let mut dbg = RecordingDebugger::new(None);
    let mut vm = Interpreter::new(&prog, &mut io, &mut dbg, ExecOptions::default()).unwrap();
    vm.run().unwrap();

    let snap = vm.debug_snapshot();
    let unico = snap.variables.iter().find(|v| v.name == "unico").unwrap();
    assert_eq!(unico.type_label, "TAluno");
    assert_eq!(unico.items.len(), 2);
    assert_eq!(unico.items[0].index_label, ".nome");
    assert_eq!(unico.items[0].value_text, "Clara");

    let turma = snap.variables.iter().find(|v| v.name == "turma").unwrap();
    assert_eq!(turma.total_items, Some(4));
    assert_eq!(turma.items[2].index_label, "[2].nome");
    assert_eq!(turma.items[2].value_text, "Sofia");
    assert_eq!(turma.items[3].index_label, "[2].nota");
    assert_eq!(turma.items[3].value_text, "10");
}

#[test]
fn handles_aleatorio_ranges_eco_and_mudacor() {
    let src = r#"algoritmo "v02_comandos"
var
   n: inteiro
   r: real
   txt: caractere
   flag: logico
   manual: inteiro
inicio
   mudacor("Azul", "FUNDOS")
   mudacor("Amarelo", "FRENTE")
   eco off
   aleatorio 10, 20
   leia(n, r, txt, flag)
   eco on
   aleatorio off
   leia(manual)
   escreval(n, "|", r, "|", compr(txt), "|", manual)
fimalgoritmo
"#;
    let mut io = MemoryIo::new(vec!["77"]);
    let mut dbg = RecordingDebugger::new(None);
    run_source(
        src,
        &mut io,
        &mut dbg,
        ExecOptions {
            rng_seed: Some(12345),
            ..Default::default()
        },
    )
    .unwrap();

    // Somente a última leitura (`manual` após `eco on`) deve ter ecoado
    assert_eq!(io.echoed_inputs, vec!["77".to_string()]);
    assert_eq!(
        io.console_colors,
        vec![
            (ConsoleColorTarget::Background, "Azul".to_string()),
            (ConsoleColorTarget::Foreground, "Amarelo".to_string()),
        ]
    );
    let line = io.output.trim();
    let parts: Vec<_> = line.split('|').collect();
    assert_eq!(parts.len(), 4);
    let n: i64 = parts[0].parse().unwrap();
    let r: f64 = parts[1].parse().unwrap();
    assert!((10..=20).contains(&n));
    assert!((10.0..=20.0).contains(&r));
    assert_eq!(parts[2], "5");
    assert_eq!(parts[3], "77");
}

#[test]
fn handles_cronometro_and_dos_safely() {
    let src = r#"algoritmo "cron_dos"
var
   i, s: inteiro
inicio
   cronometro on
   para i de 1 ate 10 faca
      s <- s + i
   fimpara
   cronometro off
   dos("echo oi")
   escreval(s)
fimalgoritmo
"#;
    let mut io = MemoryIo::new(Vec::<String>::new());
    let mut dbg = RecordingDebugger::new(None);
    run_source(src, &mut io, &mut dbg, ExecOptions::default()).unwrap();

    assert!(io.output.contains("Cronômetro iniciado.\n"));
    assert!(io.output.contains("Cronômetro terminado. Tempo decorrido:"));
    assert!(io.output.ends_with("55\n"));
    assert_eq!(io.errors.len(), 1);
    assert!(io.errors[0].contains("dos"));
}

#[test]
fn handles_pausa_and_conditional_debug_directives() {
    let src = r#"algoritmo "pausas"
var
   i: inteiro
inicio
   pausa
   para i de 1 ate 4 faca
      debug i = 3
   fimpara
fimalgoritmo
"#;
    let mut io = MemoryIo::new(Vec::<String>::new());
    let mut dbg = RecordingDebugger::new(None);
    run_source(src, &mut io, &mut dbg, ExecOptions::default()).unwrap();
    // Pausa na linha 5 (`pausa`) e uma única vez na linha 7 quando `i = 3`
    assert_eq!(dbg.paused_directive_lines, vec![5, 7]);
}

#[test]
fn handles_arquivo_write_then_read_roundtrip_and_sandbox_guards() {
    let sandbox = TempSandboxDir::new("roundtrip");
    let temp_dir = sandbox.path();

    // 1) Sem sandbox_dir -> erro claro
    let src_io = r#"algoritmo "gravacao"
arquivo "sessao.txt"
var
   nome: caractere
   nota: real
inicio
   leia(nome, nota)
   escreval(nome, "=", nota:4:1)
fimalgoritmo
"#;
    let err_no_sandbox = run_with_inputs(src_io, vec!["Marcos", "9.5"]).unwrap_err();
    assert!(err_no_sandbox.to_string().contains("salve o algoritmo"));

    // 2) Primeira execução: arquivo não existe -> grava entradas do teclado no arquivo
    let mut io_write = MemoryIo::new(vec!["Marcos", "9.5"]);
    let mut dbg1 = RecordingDebugger::new(None);
    run_source(
        src_io,
        &mut io_write,
        &mut dbg1,
        ExecOptions {
            sandbox_dir: Some(temp_dir.to_path_buf()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(io_write.output, "Marcos= 9.5\n");
    let saved_file = temp_dir.join("sessao.txt");
    assert_eq!(fs::read_to_string(&saved_file).unwrap(), "Marcos\n9.5\n");

    // 3) Segunda execução: arquivo agora existe -> lê do arquivo, e após EOF lê teclado com aviso
    let src_read_more = r#"algoritmo "leitura"
arquivo "sessao.txt"
var
   nome: caractere
   nota: real
   extra: inteiro
inicio
   leia(nome, nota, extra)
   escreval(nome, ":", nota:4:1, ":", extra)
fimalgoritmo
"#;
    let mut io_read = MemoryIo::new(vec!["99"]);
    let mut dbg2 = RecordingDebugger::new(None);
    run_source(
        src_read_more,
        &mut io_read,
        &mut dbg2,
        ExecOptions {
            sandbox_dir: Some(temp_dir.to_path_buf()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(io_read.output, "Marcos: 9.5:99\n");
    assert_eq!(io_read.errors.len(), 1);
    assert!(io_read.errors[0].contains("Fim do arquivo 'sessao.txt'"));

    // 4) Caminho absoluto fora do sandbox -> rejeitado
    let abs_bad = format!(
        "algoritmo \"bad\"\narquivo \"{}\"\nvar\n s: caractere\ninicio\n leia(s)\nfimalgoritmo\n",
        saved_file.display()
    );
    let err_abs = run_with_options(
        &abs_bad,
        Vec::<String>::new(),
        ExecOptions {
            sandbox_dir: Some(temp_dir.to_path_buf()),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(err_abs.to_string().contains("Acesso negado"));
}

#[test]
fn sandbox_blocks_traversal_symlinks_reserved_names_special_files_and_oversize_io() {
    let sandbox = TempSandboxDir::new("security");
    let outside = TempSandboxDir::new("outside");
    let dir = sandbox.path();

    let run_with_arquivo = |path_expr: &str, inputs: Vec<String>| {
        let src = format!(
            "algoritmo \"sec\"\narquivo \"{path_expr}\"\nvar\n   s: caractere\ninicio\n   leia(s)\n   escreval(s)\nfimalgoritmo\n"
        );
        run_with_options(
            &src,
            inputs,
            ExecOptions {
                sandbox_dir: Some(dir.to_path_buf()),
                ..Default::default()
            },
        )
    };

    // 1) Travessia com `..`
    fs::create_dir_all(dir.join("sub")).unwrap();
    for traversal in ["../segredo.txt", "sub/../../segredo.txt"] {
        let err = run_with_arquivo(traversal, vec!["x".to_string()]).unwrap_err();
        assert!(
            err.message.contains("Acesso negado"),
            "esperava acesso negado para '{traversal}', obteve: {err}"
        );
    }

    // 2) Nomes reservados do Windows (inclusive em subdiretório ou com extensão)
    for reserved in ["CON.txt", "NUL", "LPT1.log", "PRN", "AUX.alg", "sub/COM1.txt"] {
        let err = run_with_arquivo(reserved, vec!["x".to_string()]).unwrap_err();
        assert!(
            err.message.contains("nome reservado"),
            "esperava bloqueio de nome reservado para '{reserved}', obteve: {err}"
        );
    }

    // 3) Alvo sendo um diretório existente -> rejeitado
    fs::create_dir_all(dir.join("diretorio_alvo")).unwrap();
    let err_dir = run_with_arquivo("diretorio_alvo", Vec::new()).unwrap_err();
    assert!(err_dir.message.contains("diretório"));

    // 4) Links simbólicos (arquivo direto, interno e diretório intermediário) e FIFO no Unix
    #[cfg(unix)]
    {
        let outside_file = outside.path().join("segredo.txt");
        fs::write(&outside_file, "segredo\n").unwrap();
        std::os::unix::fs::symlink(&outside_file, dir.join("link_fora.txt")).unwrap();
        let err_sym_out = run_with_arquivo("link_fora.txt", Vec::new()).unwrap_err();
        assert!(err_sym_out.message.contains("links simbólicos"));

        let inside_file = dir.join("legit.txt");
        fs::write(&inside_file, "ok\n").unwrap();
        std::os::unix::fs::symlink(&inside_file, dir.join("link_dentro.txt")).unwrap();
        let err_sym_in = run_with_arquivo("link_dentro.txt", Vec::new()).unwrap_err();
        assert!(err_sym_in.message.contains("links simbólicos"));

        std::os::unix::fs::symlink(outside.path(), dir.join("link_dir")).unwrap();
        let err_sym_dir = run_with_arquivo("link_dir/novo.txt", Vec::new()).unwrap_err();
        assert!(err_sym_dir.message.contains("links simbólicos"));

        let fifo_path = dir.join("canal.fifo");
        if let Ok(status) = std::process::Command::new("mkfifo").arg(&fifo_path).status() {
            if status.success() {
                let err_fifo = run_with_arquivo("canal.fifo", Vec::new()).unwrap_err();
                assert!(err_fifo.message.contains("arquivo regular"));
            }
        }
    }
    #[cfg(not(unix))]
    let _ = outside;

    // 5) Leitura de arquivo > 2 MiB rejeitada antes de alocar em memória
    let huge_read = dir.join("gigante.txt");
    let huge_f = File::create(&huge_read).unwrap();
    huge_f.set_len(2 * 1024 * 1024 + 1).unwrap();
    let err_read_limit = run_with_arquivo("gigante.txt", Vec::new()).unwrap_err();
    assert!(err_read_limit.message.contains("limite de leitura"));

    // 6) Gravação > 10 MiB rejeitada com segurança
    let write_loop_src = r#"algoritmo "estouro_escrita"
arquivo "saida_grande.txt"
var
   i: inteiro
   s: caractere
inicio
   para i de 1 ate 110 faca
      leia(s)
   fimpara
fimalgoritmo
"#;
    let big_line = "a".repeat(100_000);
    let inputs = vec![big_line; 110];
    let err_write_limit = run_with_options(
        write_loop_src,
        inputs,
        ExecOptions {
            sandbox_dir: Some(dir.to_path_buf()),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(err_write_limit.message.contains("Limite máximo de tamanho"));

    // 7) Criação preguiçosa: sem `leia()`, nenhum arquivo de 0 bytes é criado (inclusive com `arquivo(...)` dentro de `var`)
    let lazy_src = r#"algoritmo "lazy"
var
   arquivo("nao_deve_existir.txt")
   x: inteiro
inicio
   x <- 10
   escreval(x)
fimalgoritmo
"#;
    let lazy_out = run_with_options(
        lazy_src,
        Vec::<String>::new(),
        ExecOptions {
            sandbox_dir: Some(dir.to_path_buf()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(lazy_out, "10\n");
    assert!(!dir.join("nao_deve_existir.txt").exists());
}

#[test]
fn guards_against_integer_overflow_extreme_arrays_and_format_dos() {
    // 1) Intervalo extremo [i64::MIN..i64::MAX] não pode passar como 0 elementos nem causar panic
    let extreme_arr = r#"algoritmo "arr_extremo"
var
   v: vetor[(-9223372036854775807 - 1)..9223372036854775807] de inteiro
inicio
fimalgoritmo
"#;
    let err_arr = run_with_inputs(extreme_arr, Vec::<String>::new()).unwrap_err();
    assert_eq!(err_arr.kind, ErrorKind::Execucao);

    // 2) `i64::MIN \ -1`, `i64::MIN % -1`, `i64::MIN mod -1` e `-i64::MIN` falham sem panic
    for expr in [
        "(-9223372036854775807 - 1) \\ -1",
        "(-9223372036854775807 - 1) % -1",
        "(-9223372036854775807 - 1) mod -1",
        "-(-9223372036854775807 - 1)",
        "abs(-9223372036854775807 - 1)",
    ] {
        let src = format!("algoritmo \"ov\"\nvar x: inteiro\ninicio\n   x <- {expr}\nfimalgoritmo\n");
        let err = run_with_inputs(&src, Vec::<String>::new()).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Execucao, "esperava erro para `{expr}`");
        assert!(
            err.message.contains("Estouro de inteiro"),
            "mensagem inesperada para `{expr}`: {}",
            err.message
        );
    }

    // 3) `copia("abc", i64::MIN, 2)` não sofre underflow nem panic
    let copia_src = r#"algoritmo "copia_min"
inicio
   escreval(copia("abc", -9223372036854775807 - 1, 2))
fimalgoritmo
"#;
    assert_eq!(run_with_inputs(copia_src, Vec::<String>::new()).unwrap(), "ab\n");

    // 4) Largura e casas decimais excessivas ou negativas na formatação falham com segurança
    let err_width = run_with_inputs(
        "algoritmo \"w\"\ninicio\n   escreval(1:10000)\nfimalgoritmo\n",
        Vec::<String>::new(),
    )
    .unwrap_err();
    assert!(err_width.message.contains("Largura de formatação"));

    let err_dec = run_with_inputs(
        "algoritmo \"d\"\ninicio\n   escreval(1.0:1:50)\nfimalgoritmo\n",
        Vec::<String>::new(),
    )
    .unwrap_err();
    assert!(err_dec.message.contains("casas decimais"));

    let err_neg_dec = run_with_inputs(
        "algoritmo \"dn\"\ninicio\n   escreval(1.0:1:-1)\nfimalgoritmo\n",
        Vec::<String>::new(),
    )
    .unwrap_err();
    assert!(err_neg_dec.message.contains("negativo"));
}

#[test]
fn guards_against_empty_tokens_deep_nesting_and_giant_expr_chains() {
    // 1) Fluxo de tokens vazio retorna erro sintático sem panic
    let empty_err = parse_program(&[]).unwrap_err();
    assert_eq!(empty_err.kind, ErrorKind::Sintatico);

    // 2) Aninhamento de parênteses > 256 retorna erro sintático sem stack overflow
    let deep_parens = format!(
        "algoritmo \"deep\"\nvar x: inteiro\ninicio\n   x <- {}1{}\nfimalgoritmo\n",
        "(".repeat(305),
        ")".repeat(305)
    );
    let deep_err = compile(&deep_parens).unwrap_err();
    assert_eq!(deep_err.kind, ErrorKind::Sintatico);
    assert!(deep_err.message.contains("aninhamento"));

    // 3) Cadeia binária gigante à esquerda (> 5.000 nós) compila, executa e é descartada sem stack overflow;
    // acima de 16.384 nós o parser rejeita por complexidade e descarta a árvore parcial com segurança.
    let chain_6000 = format!(
        "algoritmo \"chain6k\"\nconst\n   C_SUM = 1{}\nvar x: inteiro\ninicio\n   x <- 1{}\n   escreval(x, \":\", C_SUM)\nfimalgoritmo\n",
        " + 1".repeat(3_000),
        " + 1".repeat(6_000)
    );
    let out_6k = run_with_inputs(&chain_6000, Vec::<String>::new()).unwrap();
    assert_eq!(out_6k, "6001:3001\n");

    let chain_20k = format!(
        "algoritmo \"chain20k\"\nvar x: inteiro\ninicio\n   x <- 1{}\nfimalgoritmo\n",
        " + 1".repeat(20_000)
    );
    let too_complex = compile(&chain_20k).unwrap_err();
    assert_eq!(too_complex.kind, ErrorKind::Sintatico);
    assert!(too_complex.message.contains("complexidade"));

    // Drop iterativo direto de uma árvore binária à esquerda com 10.000 nós
    let span = Span::new(1, 1, 1);
    let mut left_tree = Expr::Int(1, span);
    for _ in 0..10_000 {
        left_tree = Expr::Binary {
            left: Box::new(left_tree),
            op: BinaryOp::Add,
            right: Box::new(Expr::Int(1, span)),
            span,
        };
    }
    drop(left_tree);
}

#[test]
fn preserves_nested_frame_and_locals_on_runtime_error_and_respects_eco_on_invalid_input() {
    // 1) Preserva pilha de chamadas e variáveis locais quando ocorre erro em sub-rotina
    let err_src = r#"algoritmo "PreservaQuadro"
procedimento interna(p: inteiro)
var
   acc: inteiro
inicio
   acc <- p + 35
   escreval(acc / 0)
fimprocedimento

procedimento externa(k: inteiro)
var
   local_ext: inteiro
inicio
   local_ext <- k + 5
   interna(local_ext)
fimprocedimento

inicio
   externa(15)
fimalgoritmo
"#;
    let prog = compile(err_src).unwrap();
    let mut io = MemoryIo::new(Vec::<String>::new());
    let mut dbg = RunController::noop();
    let mut vm = Interpreter::new(&prog, &mut io, &mut dbg, ExecOptions::default()).unwrap();
    let err = vm.run().unwrap_err();
    assert_eq!(err.kind, ErrorKind::Execucao);

    let snap = vm.debug_snapshot();
    assert_eq!(
        snap.call_stack,
        vec!["algoritmo \"PreservaQuadro\"", "externa", "interna"]
    );
    let vars = vm.snapshot_vars();
    assert_eq!(vars, snap.variables);
    assert!(vars
        .iter()
        .any(|v| v.scope == "interna" && v.name == "p" && v.value_text == "20"));
    assert!(vars
        .iter()
        .any(|v| v.scope == "interna" && v.name == "acc" && v.value_text == "55"));

    // 2) `eco on`/`eco off` não emite `echo_input` quando a entrada falha na validação (`strict_read = false`)
    let eco_src = r#"algoritmo "EcoValidacao"
var
   a, b: inteiro
inicio
   eco on
   leia(a)
   eco off
   leia(b)
   escreval(a, "-", b)
fimalgoritmo
"#;
    let mut eco_io = MemoryIo::new(vec!["invalido", "42", "99"]);
    let mut eco_dbg = RunController::noop();
    run_source(
        eco_src,
        &mut eco_io,
        &mut eco_dbg,
        ExecOptions {
            strict_read: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(eco_io.errors.len(), 1);
    assert_eq!(eco_io.echoed_inputs, vec!["42".to_string()]);
    assert_eq!(eco_io.output, "42-99\n");
}
