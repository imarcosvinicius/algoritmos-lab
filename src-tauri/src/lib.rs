pub mod auth_paths;
pub mod session;

use auth_paths::{read_alg_file, write_alg_file, AuthorizedPaths};
use session::{
    DebugAction, EventSink, ExecMode, OpenedFile, SessionEvent, SessionEventEnvelope,
    SessionManager, StartOptions,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::{DialogExt, FilePath};
use algoritmoslab_core::SourceEncoding;

const ALG_FILTER_NAME: &str = "Algoritmo Portugol (.alg)";
const ALG_FILTER_EXTENSIONS: &[&str] = &["alg", "ALG"];

struct TauriEventSink {
    app: AppHandle,
}

impl EventSink for TauriEventSink {
    fn emit(&self, run_id: &str, event: SessionEvent) {
        let envelope = SessionEventEnvelope {
            run_id: run_id.to_string(),
            event,
        };
        let _ = self.app.emit_to("main", "alg-session-event", envelope);
    }
}

async fn run_file_dialog<F>(op: F) -> Result<Option<PathBuf>, String>
where
    F: FnOnce() -> Option<FilePath> + Send + 'static,
{
    let picked = tauri::async_runtime::spawn_blocking(op)
        .await
        .map_err(|e| format!("Falha ao executar diálogo nativo: {e}"))?;

    picked
        .map(|fp| {
            fp.into_path()
                .map_err(|e| format!("Caminho selecionado inválido: {e}"))
        })
        .transpose()
}

#[tauri::command]
fn get_core_version() -> &'static str {
    algoritmoslab_core::crate_version()
}

#[tauri::command]
async fn pick_open_alg_path(
    app: AppHandle,
    window: tauri::Window,
    auth: State<'_, AuthorizedPaths>,
) -> Result<Option<String>, String> {
    let dialog = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("AlgoritmosLab — Abrir algoritmo (.alg)")
        .add_filter(ALG_FILTER_NAME, ALG_FILTER_EXTENSIONS);

    let Some(path) = run_file_dialog(move || dialog.blocking_pick_file()).await? else {
        return Ok(None);
    };
    let canonical = auth.authorize_open_file(&path)?;
    Ok(Some(canonical.to_string_lossy().to_string()))
}

#[tauri::command]
async fn pick_save_alg_path(
    app: AppHandle,
    window: tauri::Window,
    auth: State<'_, AuthorizedPaths>,
    default_name: Option<String>,
) -> Result<Option<String>, String> {
    let mut dialog = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("AlgoritmosLab — Salvar algoritmo (.alg)")
        .add_filter(ALG_FILTER_NAME, ALG_FILTER_EXTENSIONS);

    let mut applied_name = false;
    if let Some(raw_default) = default_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let default_path = Path::new(raw_default);
        if let Some(parent) = default_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty() && p.is_dir())
        {
            dialog = dialog.set_directory(parent);
        }
        if let Some(file_name) = default_path
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|s| !s.is_empty())
        {
            dialog = dialog.set_file_name(file_name);
            applied_name = true;
        }
    }
    if !applied_name {
        dialog = dialog.set_file_name("algoritmo.alg");
    }

    let Some(path) = run_file_dialog(move || dialog.blocking_save_file()).await? else {
        return Ok(None);
    };
    let normalized = auth.authorize_save_file(&path)?;
    Ok(Some(normalized.to_string_lossy().to_string()))
}

#[tauri::command]
async fn pick_working_dir(
    app: AppHandle,
    window: tauri::Window,
    auth: State<'_, AuthorizedPaths>,
) -> Result<Option<String>, String> {
    let dialog = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("AlgoritmosLab — Selecionar pasta de trabalho autorizada");

    let Some(path) = run_file_dialog(move || dialog.blocking_pick_folder()).await? else {
        return Ok(None);
    };
    let canonical = auth.authorize_sandbox_dir(&path)?;
    Ok(Some(canonical.to_string_lossy().to_string()))
}

#[tauri::command]
async fn open_alg_file(
    auth: State<'_, AuthorizedPaths>,
    path: String,
) -> Result<OpenedFile, String> {
    let validated = auth.validate_read_file(&path)?;
    tauri::async_runtime::spawn_blocking(move || read_alg_file(&validated))
        .await
        .map_err(|e| format!("Falha ao ler arquivo: {e}"))?
}

#[tauri::command]
async fn save_alg_file(
    auth: State<'_, AuthorizedPaths>,
    path: String,
    text: String,
    encoding: SourceEncoding,
) -> Result<(), String> {
    let validated = auth.validate_write_file(&path)?;
    tauri::async_runtime::spawn_blocking(move || write_alg_file(&validated, &text, encoding))
        .await
        .map_err(|e| format!("Falha ao salvar arquivo: {e}"))?
}

#[tauri::command]
async fn start_execution(
    app: AppHandle,
    auth: State<'_, AuthorizedPaths>,
    run_id: Option<String>,
    source: String,
    mode: ExecMode,
    breakpoints: Vec<usize>,
    options: Option<StartOptions>,
) -> Result<String, String> {
    let mut resolved_options = options.unwrap_or_default();
    if let Some(raw_dir) = resolved_options.sandbox_dir.as_deref() {
        let trimmed = raw_dir.trim();
        if trimmed.is_empty() {
            resolved_options.sandbox_dir = None;
        } else {
            let canonical = auth.validate_sandbox_dir(trimmed)?;
            resolved_options.sandbox_dir = Some(canonical.to_string_lossy().to_string());
        }
    }

    let app_clone = app.clone();
    let sink = Arc::new(TauriEventSink { app });
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app_clone.state::<SessionManager>();
        manager.start(run_id, source, mode, breakpoints, resolved_options, sink)
    })
    .await
    .map_err(|e| format!("Falha ao iniciar execução: {e}"))?
}

#[tauri::command]
fn send_input(manager: State<'_, SessionManager>, value: String) -> Result<(), String> {
    manager.send_input(value)
}

#[tauri::command]
fn debug_action(manager: State<'_, SessionManager>, action: DebugAction) -> Result<(), String> {
    manager.debug_action(action)
}

#[tauri::command]
fn update_breakpoints(manager: State<'_, SessionManager>, lines: Vec<usize>) {
    manager.update_breakpoints(lines);
}

#[tauri::command]
fn stop_execution(manager: State<'_, SessionManager>) {
    manager.stop();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .manage(AuthorizedPaths::new())
        .manage(SessionManager::new())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_core_version,
            pick_open_alg_path,
            pick_save_alg_path,
            pick_working_dir,
            open_alg_file,
            save_alg_file,
            start_execution,
            send_input,
            debug_action,
            update_breakpoints,
            stop_execution,
        ])
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o AlgoritmosLab");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            if let Some(mgr) = app_handle.try_state::<SessionManager>() {
                mgr.stop_and_join();
            }
        }
    });
}
