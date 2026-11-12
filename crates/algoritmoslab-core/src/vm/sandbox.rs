use crate::encoding::decode_source_bytes;
use crate::error::{Result, Span, AlgError};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};

pub(crate) const MAX_ARQUIVO_READ_BYTES: u64 = 2 * 1024 * 1024;
pub(crate) const MAX_ARQUIVO_WRITE_BYTES: u64 = 10 * 1024 * 1024;
pub(crate) const MAX_ARQUIVO_WRITE_LINES: usize = 200_000;

pub(crate) enum FileMode {
    None,
    Reading {
        display_name: String,
        lines: VecDeque<String>,
        line_no: usize,
    },
    PendingWrite {
        display_name: String,
        sandbox_base: PathBuf,
        resolved: PathBuf,
    },
    Writing {
        display_name: String,
        writer: BufWriter<File>,
        bytes_written: u64,
        lines_written: usize,
    },
}

pub(crate) fn is_windows_reserved_name(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches(' ');
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

pub(crate) fn resolve_sandbox_path(
    base_dir: &Path,
    raw_path: &str,
    span: Span,
) -> Result<(PathBuf, PathBuf)> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() || trimmed.contains('\0') {
        return Err(AlgError::runtime(
            Some(span),
            "Nome de arquivo inválido no comando 'arquivo'.",
        ));
    }

    let normalized = trimmed.replace('\\', "/");
    if normalized.starts_with('/') || normalized.contains(':') {
        return Err(AlgError::runtime(
            Some(span),
            format!(
                "Acesso negado: o comando 'arquivo' aceita apenas caminhos relativos dentro da pasta autorizada (recebido '{raw_path}')."
            ),
        ));
    }

    let mut rel = PathBuf::new();
    for comp in Path::new(&normalized).components() {
        match comp {
            Component::Normal(part) => {
                let part_str = part.to_string_lossy();
                if is_windows_reserved_name(&part_str) {
                    return Err(AlgError::runtime(
                        Some(span),
                        format!(
                            "Acesso negado: nome reservado do sistema não é permitido no comando 'arquivo' ('{raw_path}')."
                        ),
                    ));
                }
                rel.push(part);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AlgError::runtime(
                    Some(span),
                    format!(
                        "Acesso negado: caminho '{raw_path}' tenta sair da pasta autorizada (sandbox)."
                    ),
                ));
            }
        }
    }

    if rel.as_os_str().is_empty() {
        return Err(AlgError::runtime(
            Some(span),
            "Nome de arquivo inválido no comando 'arquivo'.",
        ));
    }

    let canon_base = std::fs::canonicalize(base_dir).map_err(|e| {
        AlgError::runtime(
            Some(span),
            format!("Pasta autorizada para 'arquivo' indisponível: {e}"),
        )
    })?;
    if !canon_base.is_dir() {
        return Err(AlgError::runtime(
            Some(span),
            "O caminho de trabalho configurado para 'arquivo' não é um diretório.",
        ));
    }

    // Verifica que nenhum diretório intermediário existente seja symlink
    let mut cursor = canon_base.clone();
    let mut comps = rel.components().peekable();
    while let Some(comp) = comps.next() {
        if let Component::Normal(part) = comp {
            cursor.push(part);
            if comps.peek().is_some() {
                let meta = std::fs::symlink_metadata(&cursor).map_err(|e| {
                    AlgError::runtime(
                        Some(span),
                        format!("Pasta destino inválida para '{raw_path}': {e}"),
                    )
                })?;
                if meta.file_type().is_symlink() {
                    return Err(AlgError::runtime(
                        Some(span),
                        "Acesso negado: links simbólicos não são permitidos no comando 'arquivo'.",
                    ));
                }
            }
        }
    }

    let candidate = canon_base.join(&rel);
    if let Ok(meta) = std::fs::symlink_metadata(&candidate) {
        if meta.file_type().is_symlink() {
            return Err(AlgError::runtime(
                Some(span),
                "Acesso negado: links simbólicos não são permitidos no comando 'arquivo'.",
            ));
        }
        let canon_target = std::fs::canonicalize(&candidate).map_err(|e| {
            AlgError::runtime(Some(span), format!("Caminho inválido em 'arquivo': {e}"))
        })?;
        if !canon_target.starts_with(&canon_base) {
            return Err(AlgError::runtime(
                Some(span),
                "Acesso negado: caminho fora da pasta autorizada.",
            ));
        }
        return Ok((canon_base, canon_target));
    }

    let parent = candidate.parent().unwrap_or(&canon_base);
    let canon_parent = std::fs::canonicalize(parent).map_err(|e| {
        AlgError::runtime(
            Some(span),
            format!("Pasta destino inválida para '{raw_path}': {e}"),
        )
    })?;
    if !canon_parent.starts_with(&canon_base) {
        return Err(AlgError::runtime(
            Some(span),
            "Acesso negado: pasta destino fora da área autorizada.",
        ));
    }

    let file_name = candidate.file_name().ok_or_else(|| {
        AlgError::runtime(
            Some(span),
            "Nome de arquivo inválido no comando 'arquivo'.",
        )
    })?;

    Ok((canon_base, canon_parent.join(file_name)))
}

pub(crate) fn activate_arquivo(
    sandbox_dir: Option<&Path>,
    file_mode: &mut FileMode,
    raw_path: &str,
    span: Span,
) -> Result<()> {
    let base_dir = sandbox_dir.ok_or_else(|| {
        AlgError::runtime(
            Some(span),
            "Para usar o comando 'arquivo', salve o algoritmo (.alg) antes ou selecione uma pasta de trabalho autorizada.",
        )
    })?;

    let (canon_base, resolved) = resolve_sandbox_path(base_dir, raw_path, span)?;
    flush_file_writer(file_mode);

    match std::fs::symlink_metadata(&resolved) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(AlgError::runtime(
                    Some(span),
                    "Acesso negado: links simbólicos não são permitidos no comando 'arquivo'.",
                ));
            }
            if meta.is_dir() {
                return Err(AlgError::runtime(
                    Some(span),
                    format!("'{raw_path}' é um diretório, esperado arquivo de texto."),
                ));
            }
            if !meta.file_type().is_file() {
                return Err(AlgError::runtime(
                    Some(span),
                    format!(
                        "Acesso negado: '{raw_path}' não é um arquivo regular."
                    ),
                ));
            }
            if meta.len() > MAX_ARQUIVO_READ_BYTES {
                return Err(AlgError::runtime(
                    Some(span),
                    format!(
                        "Arquivo '{raw_path}' excede o limite de leitura ({MAX_ARQUIVO_READ_BYTES} bytes)."
                    ),
                ));
            }

            let file = File::open(&resolved).map_err(|e| {
                AlgError::runtime(
                    Some(span),
                    format!("Não foi possível ler o arquivo '{raw_path}': {e}"),
                )
            })?;
            verify_opened_regular_file(&file, &meta, span)?;

            let mut bytes = Vec::with_capacity((meta.len() as usize).min(MAX_ARQUIVO_READ_BYTES as usize));
            file.take(MAX_ARQUIVO_READ_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| {
                    AlgError::runtime(
                        Some(span),
                        format!("Não foi possível ler o arquivo '{raw_path}': {e}"),
                    )
                })?;
            if bytes.len() as u64 > MAX_ARQUIVO_READ_BYTES {
                return Err(AlgError::runtime(
                    Some(span),
                    format!(
                        "Arquivo '{raw_path}' excede o limite de leitura ({MAX_ARQUIVO_READ_BYTES} bytes)."
                    ),
                ));
            }

            let decoded = decode_source_bytes(&bytes);
            let lines: VecDeque<String> = decoded
                .text
                .lines()
                .map(ToString::to_string)
                .collect();
            *file_mode = FileMode::Reading {
                display_name: raw_path.to_string(),
                lines,
                line_no: 0,
            };
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            *file_mode = FileMode::PendingWrite {
                display_name: raw_path.to_string(),
                sandbox_base: canon_base,
                resolved,
            };
        }
        Err(e) => {
            return Err(AlgError::runtime(
                Some(span),
                format!("Falha ao acessar arquivo '{raw_path}': {e}"),
            ));
        }
    }

    Ok(())
}

fn verify_opened_regular_file(
    file: &File,
    lstat_meta: &std::fs::Metadata,
    span: Span,
) -> Result<()> {
    let opened_meta = file.metadata().map_err(|e| {
        AlgError::runtime(
            Some(span),
            format!("Falha ao verificar metadados do arquivo aberto: {e}"),
        )
    })?;
    if !opened_meta.file_type().is_file() {
        return Err(AlgError::runtime(
            Some(span),
            "Acesso negado: o alvo aberto não é um arquivo regular.",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened_meta.dev() != lstat_meta.dev() || opened_meta.ino() != lstat_meta.ino() {
            return Err(AlgError::runtime(
                Some(span),
                "Acesso negado: o arquivo foi alterado ou substituído de forma insegura durante a abertura.",
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = lstat_meta;
    Ok(())
}

pub(crate) fn write_line_to_arquivo(
    file_mode: &mut FileMode,
    line: &str,
    span: Span,
) -> Result<()> {
    if let FileMode::PendingWrite {
        display_name,
        sandbox_base,
        resolved,
    } = file_mode
    {
        let parent = resolved.parent().ok_or_else(|| {
            AlgError::runtime(Some(span), "Caminho destino inválido para gravação.")
        })?;
        let parent_meta = std::fs::symlink_metadata(parent).map_err(|e| {
            AlgError::runtime(
                Some(span),
                format!("Falha ao validar pasta destino de '{display_name}': {e}"),
            )
        })?;
        if parent_meta.file_type().is_symlink() {
            return Err(AlgError::runtime(
                Some(span),
                "Acesso negado: pasta destino contém link simbólico.",
            ));
        }
        let canon_parent = std::fs::canonicalize(parent).map_err(|e| {
            AlgError::runtime(
                Some(span),
                format!("Falha ao resolver pasta destino de '{display_name}': {e}"),
            )
        })?;
        if !canon_parent.starts_with(sandbox_base) {
            return Err(AlgError::runtime(
                Some(span),
                "Acesso negado: pasta destino fora da área autorizada.",
            ));
        }

        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(resolved)
            .map_err(|e| {
                AlgError::runtime(
                    Some(span),
                    format!("Não foi possível criar o arquivo '{display_name}': {e}"),
                )
            })?;
        *file_mode = FileMode::Writing {
            display_name: display_name.clone(),
            writer: BufWriter::new(file),
            bytes_written: 0,
            lines_written: 0,
        };
    }

    if let FileMode::Writing {
        display_name,
        writer,
        bytes_written,
        lines_written,
    } = file_mode
    {
        if *lines_written >= MAX_ARQUIVO_WRITE_LINES {
            return Err(AlgError::runtime(
                Some(span),
                format!(
                    "Limite máximo de linhas ({MAX_ARQUIVO_WRITE_LINES}) atingido ao gravar em '{display_name}'."
                ),
            ));
        }
        let added_bytes = (line.len() as u64).saturating_add(1);
        if bytes_written.saturating_add(added_bytes) > MAX_ARQUIVO_WRITE_BYTES {
            return Err(AlgError::runtime(
                Some(span),
                format!(
                    "Limite máximo de tamanho ({MAX_ARQUIVO_WRITE_BYTES} bytes) atingido ao gravar em '{display_name}'."
                ),
            ));
        }
        writeln!(writer, "{line}")
            .and_then(|_| writer.flush())
            .map_err(|e| {
                AlgError::runtime(
                    Some(span),
                    format!("Falha ao gravar no arquivo '{display_name}': {e}"),
                )
            })?;
        *bytes_written += added_bytes;
        *lines_written += 1;
    }

    Ok(())
}

pub(crate) fn flush_file_writer(file_mode: &mut FileMode) {
    if let FileMode::Writing { writer, .. } = file_mode {
        let _ = writer.flush();
    }
}
