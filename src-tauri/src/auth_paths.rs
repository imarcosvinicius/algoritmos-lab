use crate::session::OpenedFile;
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use algoritmoslab_core::{decode_source_bytes, encode_source, SourceEncoding};

pub const MAX_ALG_FILE_BYTES: u64 = 4 * 1024 * 1024;

static TEMP_WRITE_SEQ: AtomicU64 = AtomicU64::new(1);

pub(crate) fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Default)]
struct AuthPathSet {
    read_files: HashSet<PathBuf>,
    write_files: HashSet<PathBuf>,
    sandbox_dirs: HashSet<PathBuf>,
}

#[derive(Default)]
pub struct AuthorizedPaths {
    inner: Mutex<AuthPathSet>,
}

impl AuthorizedPaths {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn authorize_open_file(&self, path: &Path) -> Result<PathBuf, String> {
        let canonical = normalize_existing_file(path)?;
        let canonical_parent = canonical
            .parent()
            .ok_or_else(|| "O arquivo selecionado não possui diretório pai válido.".to_string())
            .and_then(normalize_existing_dir)?;

        let mut set = lock_unpoisoned(&self.inner);
        set.read_files.insert(canonical.clone());
        set.write_files.insert(canonical.clone());
        set.sandbox_dirs.insert(canonical_parent);
        Ok(canonical)
    }

    pub fn authorize_save_file(&self, path: &Path) -> Result<PathBuf, String> {
        let normalized = normalize_save_target(path)?;
        let canonical_parent = normalized
            .parent()
            .ok_or_else(|| "O destino de gravação não possui diretório pai válido.".to_string())
            .and_then(normalize_existing_dir)?;

        let mut set = lock_unpoisoned(&self.inner);
        set.write_files.insert(normalized.clone());
        set.read_files.insert(normalized.clone());
        set.sandbox_dirs.insert(canonical_parent);
        Ok(normalized)
    }

    pub fn authorize_sandbox_dir(&self, dir: &Path) -> Result<PathBuf, String> {
        let canonical = normalize_existing_dir(dir)?;
        let mut set = lock_unpoisoned(&self.inner);
        set.sandbox_dirs.insert(canonical.clone());
        Ok(canonical)
    }

    pub fn validate_read_file(&self, raw: &str) -> Result<PathBuf, String> {
        if raw.trim().is_empty() {
            return Err("Caminho de arquivo vazio.".to_string());
        }
        let canonical = normalize_existing_file(Path::new(raw))?;
        let set = lock_unpoisoned(&self.inner);
        if set.read_files.contains(&canonical) {
            Ok(canonical)
        } else {
            Err("Acesso negado: o arquivo não foi autorizado pelo usuário.".to_string())
        }
    }

    pub fn validate_write_file(&self, raw: &str) -> Result<PathBuf, String> {
        if raw.trim().is_empty() {
            return Err("Caminho de destino vazio.".to_string());
        }
        let normalized = normalize_save_target(Path::new(raw))?;
        let set = lock_unpoisoned(&self.inner);
        if set.write_files.contains(&normalized) {
            Ok(normalized)
        } else {
            Err("Acesso negado: o destino de gravação não foi autorizado pelo usuário.".to_string())
        }
    }

    pub fn validate_sandbox_dir(&self, raw: &str) -> Result<PathBuf, String> {
        if raw.trim().is_empty() {
            return Err(
                "Pasta de trabalho não autorizada. Selecione a pasta pela interface do aplicativo."
                    .to_string(),
            );
        }
        let canonical = normalize_existing_dir(Path::new(raw))?;
        let set = lock_unpoisoned(&self.inner);
        if set.sandbox_dirs.contains(&canonical) {
            Ok(canonical)
        } else {
            Err(
                "Pasta de trabalho não autorizada. Selecione a pasta pela interface do aplicativo."
                    .to_string(),
            )
        }
    }
}

fn reject_trailing_separator(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("Caminho vazio não é permitido.".to_string());
    }
    let raw = path.to_string_lossy();
    if raw.chars().next_back().is_some_and(std::path::is_separator) && path.parent().is_some() {
        return Err(
            "O caminho especificado aponta para um diretório, não para um arquivo.".to_string(),
        );
    }
    Ok(())
}

fn inspect_regular_non_symlink(
    path: &Path,
    inspect_verb: &str,
    not_file_msg: &str,
) -> Result<std::fs::Metadata, String> {
    let meta = std::fs::symlink_metadata(path)
        .map_err(|e| format!("Não foi possível {inspect_verb} '{}': {e}", path.display()))?;
    if meta.file_type().is_symlink() {
        return Err(format!(
            "Links simbólicos não são permitidos ('{}').",
            path.display()
        ));
    }
    if !meta.file_type().is_file() {
        return Err(format!("{not_file_msg} ('{}').", path.display()));
    }
    Ok(meta)
}

fn inspect_dir_non_symlink(path: &Path, inspect_label: &str, not_dir_msg: &str) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| {
        format!(
            "Não foi possível inspecionar {inspect_label}'{}': {e}",
            path.display()
        )
    })?;
    if meta.file_type().is_symlink() || !meta.file_type().is_dir() {
        return Err(format!("{not_dir_msg} ('{}').", path.display()));
    }
    Ok(())
}

pub fn normalize_existing_file(path: &Path) -> Result<PathBuf, String> {
    reject_trailing_separator(path)?;

    let link_meta = inspect_regular_non_symlink(
        path,
        "inspecionar",
        "O caminho especificado não é um arquivo regular",
    )?;

    let canonical = std::fs::canonicalize(path)
        .map_err(|e| format!("Não foi possível resolver '{}': {e}", path.display()))?;
    let canon_meta = inspect_regular_non_symlink(
        &canonical,
        "inspecionar",
        "O caminho resolvido não é um arquivo regular válido",
    )?;

    #[cfg(unix)]
    if (link_meta.dev(), link_meta.ino()) != (canon_meta.dev(), canon_meta.ino()) {
        return Err("Alteração concorrente detectada ao resolver o arquivo.".to_string());
    }

    Ok(canonical)
}

pub fn normalize_save_target(path: &Path) -> Result<PathBuf, String> {
    reject_trailing_separator(path)?;

    let file_name = path
        .file_name()
        .filter(|n| !n.is_empty())
        .ok_or_else(|| "Caminho de destino sem nome de arquivo válido.".to_string())?;

    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(format!(
                    "Links simbólicos não são permitidos como destino de gravação ('{}').",
                    path.display()
                ));
            }
            if meta.is_dir() {
                return Err(format!(
                    "O destino aponta para uma pasta, não para um arquivo ('{}').",
                    path.display()
                ));
            }
            if !meta.file_type().is_file() {
                return Err(format!(
                    "O destino especificado não é um arquivo regular ('{}').",
                    path.display()
                ));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(format!(
                "Não foi possível inspecionar o destino '{}': {e}",
                path.display()
            ));
        }
    }

    let raw_parent = path
        .parent()
        .ok_or_else(|| "Caminho de destino sem diretório pai válido.".to_string())?;
    let parent_to_resolve = if raw_parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        raw_parent
    };

    let canonical_parent = std::fs::canonicalize(parent_to_resolve).map_err(|e| {
        format!(
            "Não foi possível resolver o diretório pai de '{}': {e}",
            path.display()
        )
    })?;
    inspect_dir_non_symlink(
        &canonical_parent,
        "o diretório pai ",
        "O diretório pai resolvido não é uma pasta válida",
    )?;

    Ok(canonical_parent.join(file_name))
}

pub fn normalize_existing_dir(dir: &Path) -> Result<PathBuf, String> {
    if dir.as_os_str().is_empty() {
        return Err("Caminho de pasta vazio.".to_string());
    }
    let canonical = std::fs::canonicalize(dir)
        .map_err(|e| format!("Não foi possível resolver a pasta '{}': {e}", dir.display()))?;
    inspect_dir_non_symlink(
        &canonical,
        "",
        "O caminho especificado não é uma pasta válida",
    )?;
    Ok(canonical)
}

pub fn read_alg_file(path: &Path) -> Result<OpenedFile, String> {
    let link_meta = inspect_regular_non_symlink(
        path,
        "abrir",
        "O caminho especificado não é um arquivo regular",
    )?;
    if link_meta.len() > MAX_ALG_FILE_BYTES {
        return Err(format!(
            "O arquivo '{}' excede o limite máximo de {} bytes.",
            path.display(),
            MAX_ALG_FILE_BYTES
        ));
    }

    let file = File::open(path)
        .map_err(|e| format!("Não foi possível abrir '{}': {e}", path.display()))?;
    let open_meta = file.metadata().map_err(|e| {
        format!(
            "Não foi possível ler metadados de '{}': {e}",
            path.display()
        )
    })?;
    if !open_meta.file_type().is_file() {
        return Err(format!(
            "O arquivo aberto não é um arquivo regular ('{}').",
            path.display()
        ));
    }
    if open_meta.len() > MAX_ALG_FILE_BYTES {
        return Err(format!(
            "O arquivo '{}' excede o limite máximo de {} bytes.",
            path.display(),
            MAX_ALG_FILE_BYTES
        ));
    }

    #[cfg(unix)]
    if (link_meta.dev(), link_meta.ino()) != (open_meta.dev(), open_meta.ino()) {
        return Err("Alteração concorrente detectada durante a abertura do arquivo.".to_string());
    }

    let capacity = usize::try_from(open_meta.len().min(MAX_ALG_FILE_BYTES)).unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    file.take(MAX_ALG_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Não foi possível ler '{}': {e}", path.display()))?;

    if bytes.len() as u64 > MAX_ALG_FILE_BYTES {
        return Err(format!(
            "O arquivo '{}' excede o limite máximo de {} bytes.",
            path.display(),
            MAX_ALG_FILE_BYTES
        ));
    }

    let decoded = decode_source_bytes(&bytes);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("algoritmo.alg")
        .to_string();

    Ok(OpenedFile {
        path: path.to_string_lossy().to_string(),
        name,
        text: decoded.text,
        encoding: decoded.encoding,
    })
}

pub fn write_alg_file(path: &Path, text: &str, encoding: SourceEncoding) -> Result<(), String> {
    if text.len() as u64 > MAX_ALG_FILE_BYTES {
        return Err(format!(
            "O conteúdo excede o limite máximo de {} bytes.",
            MAX_ALG_FILE_BYTES
        ));
    }

    let bytes = encode_source(text, encoding);
    if bytes.len() as u64 > MAX_ALG_FILE_BYTES {
        return Err(format!(
            "O conteúdo codificado excede o limite máximo de {} bytes.",
            MAX_ALG_FILE_BYTES
        ));
    }

    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(format!(
                    "Links simbólicos não são permitidos como destino de gravação ('{}').",
                    path.display()
                ));
            }
            if meta.is_dir() {
                return Err(format!(
                    "O destino aponta para uma pasta, não para um arquivo ('{}').",
                    path.display()
                ));
            }
            if !meta.file_type().is_file() {
                return Err(format!(
                    "O destino especificado não é um arquivo regular ('{}').",
                    path.display()
                ));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(format!(
                "Não foi possível inspecionar '{}': {e}",
                path.display()
            ));
        }
    }

    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let seq = TEMP_WRITE_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_path = parent.join(format!(
        ".algoritmoslab-write-{}-{seq}.tmp",
        std::process::id()
    ));

    let mut tmp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .map_err(|e| format!("Não foi possível salvar '{}': {e}", path.display()))?;

    let write_result = tmp_file.write_all(&bytes).and_then(|()| tmp_file.flush());

    drop(tmp_file);

    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("Não foi possível gravar '{}': {e}", path.display()));
    }

    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("Não foi possível salvar '{}': {e}", path.display()));
    }

    Ok(())
}
