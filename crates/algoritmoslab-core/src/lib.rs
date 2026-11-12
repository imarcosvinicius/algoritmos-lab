//! Núcleo do interpretador e depurador de Portugol estruturado (.alg) do AlgoritmosLab.

pub mod ast;
pub mod builtins;
pub mod encoding;
pub mod error;
pub mod golden;
pub mod lexer;
pub mod parser;
pub mod token;
pub mod value;
pub mod vm;

pub use encoding::{decode_source_bytes, encode_source, DecodedSource, SourceEncoding};
pub use error::{ErrorKind, Result, Span, AlgError};
pub use value::{Value, VarArrayItem, VarSnapshot};
pub use vm::{
    ConsoleColorTarget, DebugController, DebugSnapshot, ExecOptions, Interpreter, IoHandler,
    MemoryIo, RunController,
};

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Faz o parsing completo de um código-fonte de Portugol estruturado (`.alg`).
pub fn compile(source: &str) -> Result<ast::Program> {
    let tokens = lexer::tokenize(source)?;
    parser::parse_program(&tokens)
}

/// Executa um código-fonte `.alg` completo usando o I/O e depurador fornecidos.
pub fn run_source(
    source: &str,
    io: &mut dyn IoHandler,
    debug: &mut dyn DebugController,
    options: ExecOptions,
) -> Result<()> {
    let program = compile(source)?;
    let mut vm = Interpreter::new(&program, io, debug, options)?;
    vm.run()
}

/// Executa um algoritmo em memória com entradas pré-definidas (útil para CLI e testes Golden).
pub fn run_with_inputs<I, S>(source: &str, inputs: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    run_with_options(source, inputs, ExecOptions::default())
}

/// Executa um algoritmo em memória com entradas pré-definidas e opções explícitas.
pub fn run_with_options<I, S>(source: &str, inputs: I, options: ExecOptions) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut io = MemoryIo::new(inputs);
    let mut debug = RunController::noop();
    run_source(source, &mut io, &mut debug, options)?;
    Ok(io.output)
}
