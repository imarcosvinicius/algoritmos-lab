use crate::auth_paths::lock_unpoisoned;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use algoritmoslab_core::{
    compile, ConsoleColorTarget, DebugController, DebugSnapshot, ErrorKind, ExecOptions,
    Interpreter, IoHandler, SourceEncoding, VarSnapshot, AlgError,
};

const OUTPUT_BATCH_MAX_BYTES: usize = 16 * 1024;
const OUTPUT_FLUSH_INTERVAL: Duration = Duration::from_millis(16);
const IPC_WINDOW_DURATION: Duration = Duration::from_millis(250);
const IPC_WINDOW_MAX_EVENTS: usize = 32;
const IPC_WINDOW_MAX_BYTES: usize = 128 * 1024;
const IPC_THROTTLE_PAUSE: Duration = Duration::from_millis(16);

fn wait_unpoisoned<'a, T>(cvar: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    cvar.wait(guard)
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecMode {
    Run,
    StepInto,
    StepOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebugAction {
    StepInto,
    StepOver,
    Continue,
    Pause,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct StartOptions {
    #[serde(default)]
    pub strict_read: bool,
    #[serde(default)]
    pub sandbox_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenedFile {
    pub path: String,
    pub name: String,
    pub text: String,
    pub encoding: SourceEncoding,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionEventEnvelope {
    pub run_id: String,
    pub event: SessionEvent,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    Output {
        text: String,
    },
    InputEcho {
        text: String,
    },
    ConsoleColor {
        target: String,
        color: String,
    },
    Clear,
    ErrorNote {
        message: String,
    },
    TraceLine {
        line: usize,
        variables: Vec<VarSnapshot>,
    },
    AwaitingInput {
        var_name: String,
        expected_type: String,
        line: usize,
    },
    Paused {
        line: usize,
        call_depth: usize,
        call_stack: Vec<String>,
        variables: Vec<VarSnapshot>,
    },
    Finished {
        variables: Vec<VarSnapshot>,
    },
    Error {
        kind: ErrorKind,
        line: Option<usize>,
        column: Option<usize>,
        message: String,
        variables: Vec<VarSnapshot>,
    },
}

pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, run_id: &str, event: SessionEvent);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InternalMode {
    Run,
    StepInto,
    StepOver { target_depth: usize },
    PauseRequested,
}

struct ControlState {
    mode: InternalMode,
    paused: bool,
    paused_depth: Option<usize>,
    finished: bool,
    breakpoints: HashSet<usize>,
    pending_input: VecDeque<String>,
}

struct SharedControl {
    cancel: AtomicBool,
    state: Mutex<ControlState>,
    cvar: Condvar,
}

impl SharedControl {
    fn new(mode: ExecMode, breakpoints: Vec<usize>) -> Self {
        let internal = match mode {
            ExecMode::Run => InternalMode::Run,
            ExecMode::StepInto => InternalMode::StepInto,
            ExecMode::StepOver => InternalMode::StepOver { target_depth: 0 },
        };
        Self {
            cancel: AtomicBool::new(false),
            state: Mutex::new(ControlState {
                mode: internal,
                paused: false,
                paused_depth: None,
                finished: false,
                breakpoints: breakpoints.into_iter().collect(),
                pending_input: VecDeque::new(),
            }),
            cvar: Condvar::new(),
        }
    }

    fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        let mut st = lock_unpoisoned(&self.state);
        st.paused = false;
        st.paused_depth = None;
        self.cvar.notify_all();
    }

    fn mark_finished(&self) {
        let mut st = lock_unpoisoned(&self.state);
        st.finished = true;
        st.paused = false;
        st.paused_depth = None;
        self.cvar.notify_all();
    }

    fn send_input(&self, value: String) -> Result<(), String> {
        let mut st = lock_unpoisoned(&self.state);
        if st.finished || self.cancel.load(Ordering::SeqCst) {
            return Err("A execução já foi encerrada.".to_string());
        }
        st.pending_input.push_back(value);
        self.cvar.notify_all();
        Ok(())
    }

    fn apply_action(&self, action: DebugAction) -> Result<(), String> {
        let mut st = lock_unpoisoned(&self.state);
        if st.finished || self.cancel.load(Ordering::SeqCst) {
            return Err("A execução já foi encerrada.".to_string());
        }
        match action {
            DebugAction::Pause => {
                if !st.paused {
                    st.mode = InternalMode::PauseRequested;
                }
            }
            DebugAction::Continue => {
                st.mode = InternalMode::Run;
                st.paused = false;
                st.paused_depth = None;
                self.cvar.notify_all();
            }
            DebugAction::StepInto => {
                if !st.paused {
                    return Err(
                        "Depuração passo a passo exige que a execução esteja pausada.".to_string(),
                    );
                }
                st.mode = InternalMode::StepInto;
                st.paused = false;
                st.paused_depth = None;
                self.cvar.notify_all();
            }
            DebugAction::StepOver => {
                if !st.paused {
                    return Err(
                        "Depuração passo a passo exige que a execução esteja pausada.".to_string(),
                    );
                }
                let target_depth = st.paused_depth.unwrap_or(0);
                st.mode = InternalMode::StepOver { target_depth };
                st.paused = false;
                st.paused_depth = None;
                self.cvar.notify_all();
            }
        }
        Ok(())
    }

    fn set_breakpoints(&self, lines: Vec<usize>) {
        let mut st = lock_unpoisoned(&self.state);
        st.breakpoints = lines.into_iter().collect();
    }

    fn sleep_interruptible(&self, delay: Duration) {
        if self.cancel.load(Ordering::SeqCst) {
            return;
        }
        let st = lock_unpoisoned(&self.state);
        if self.cancel.load(Ordering::SeqCst) {
            return;
        }
        let _ = self.cvar.wait_timeout(st, delay);
    }
}

struct OutputBatcher {
    buffer: String,
    last_flush: Instant,
    window_start: Instant,
    window_events: usize,
    window_bytes: usize,
}

impl OutputBatcher {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            buffer: String::new(),
            last_flush: now,
            window_start: now,
            window_events: 0,
            window_bytes: 0,
        }
    }

    fn push(&mut self, text: &str) -> Option<(String, Option<Duration>)> {
        self.buffer.push_str(text);
        let elapsed = self.last_flush.elapsed();
        if self.buffer.len() >= OUTPUT_BATCH_MAX_BYTES
            || (!self.buffer.is_empty() && elapsed >= OUTPUT_FLUSH_INTERVAL)
        {
            self.take_flush()
        } else {
            None
        }
    }

    fn take_flush(&mut self) -> Option<(String, Option<Duration>)> {
        self.last_flush = Instant::now();
        if self.buffer.is_empty() {
            return None;
        }
        let text = std::mem::take(&mut self.buffer);
        let throttle = self.record_ipc_event(text.len());
        Some((text, throttle))
    }

    fn clear(&mut self) -> Option<Duration> {
        self.buffer.clear();
        self.last_flush = Instant::now();
        self.record_ipc_event(0)
    }

    fn record_ipc_event(&mut self, bytes: usize) -> Option<Duration> {
        let now = Instant::now();
        if now.duration_since(self.window_start) >= IPC_WINDOW_DURATION {
            self.window_start = now;
            self.window_events = 0;
            self.window_bytes = 0;
        }
        self.window_events = self.window_events.saturating_add(1);
        self.window_bytes = self.window_bytes.saturating_add(bytes);

        if self.window_events > IPC_WINDOW_MAX_EVENTS || self.window_bytes > IPC_WINDOW_MAX_BYTES {
            Some(IPC_THROTTLE_PAUSE)
        } else {
            None
        }
    }
}

#[derive(Clone)]
struct BridgeContext {
    run_id: String,
    control: Arc<SharedControl>,
    sink: Arc<dyn EventSink>,
    batcher: Rc<RefCell<OutputBatcher>>,
}

impl BridgeContext {
    fn emit(&self, event: SessionEvent) {
        self.sink.emit(&self.run_id, event);
    }

    fn check_cancel(&self) -> algoritmoslab_core::Result<()> {
        if self.control.cancel.load(Ordering::SeqCst) {
            Err(AlgError::interrupted())
        } else {
            Ok(())
        }
    }

    fn emit_pending_output(&self, pending: Option<(String, Option<Duration>)>) {
        if let Some((chunk, throttle)) = pending {
            self.emit(SessionEvent::Output { text: chunk });
            if let Some(delay) = throttle {
                self.control.sleep_interruptible(delay);
            }
        }
    }

    fn push_output(&self, text: &str) {
        let pending = {
            let mut batcher = self.batcher.borrow_mut();
            batcher.push(text)
        };
        self.emit_pending_output(pending);
    }

    fn flush_output(&self) {
        let pending = {
            let mut batcher = self.batcher.borrow_mut();
            batcher.take_flush()
        };
        self.emit_pending_output(pending);
    }

    fn clear_output(&self) {
        let throttle = {
            let mut batcher = self.batcher.borrow_mut();
            batcher.clear()
        };
        self.emit(SessionEvent::Clear);
        if let Some(delay) = throttle {
            self.control.sleep_interruptible(delay);
        }
    }
}

struct ActiveSession {
    control: Arc<SharedControl>,
    join_handle: Option<JoinHandle<()>>,
}

pub struct SessionManager {
    active: Arc<Mutex<Option<ActiveSession>>>,
    lifecycle_lock: Mutex<()>,
    seq: AtomicU64,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            active: Arc::new(Mutex::new(None)),
            lifecycle_lock: Mutex::new(()),
            seq: AtomicU64::new(1),
        }
    }

    fn with_active_control(&self) -> Option<Arc<SharedControl>> {
        let guard = lock_unpoisoned(&self.active);
        guard.as_ref().map(|session| Arc::clone(&session.control))
    }

    fn take_and_join_active(&self) {
        let prev = {
            let mut guard = lock_unpoisoned(&self.active);
            guard.take()
        };
        if let Some(session) = prev {
            session.control.cancel();
            if let Some(handle) = session.join_handle {
                let _ = handle.join();
            }
        }
    }

    pub fn start(
        &self,
        run_id: Option<String>,
        source: String,
        mode: ExecMode,
        breakpoints: Vec<usize>,
        options: StartOptions,
        sink: Arc<dyn EventSink>,
    ) -> Result<String, String> {
        let _lifecycle = lock_unpoisoned(&self.lifecycle_lock);
        self.take_and_join_active();

        let resolved_id = run_id.unwrap_or_else(|| {
            let n = self.seq.fetch_add(1, Ordering::Relaxed);
            format!("session-{n}")
        });

        let control = Arc::new(SharedControl::new(mode, breakpoints));
        {
            let mut guard = lock_unpoisoned(&self.active);
            *guard = Some(ActiveSession {
                control: Arc::clone(&control),
                join_handle: None,
            });
        }

        let control_bg = Arc::clone(&control);
        let active_weak = Arc::downgrade(&self.active);
        let thread_run_id = resolved_id.clone();

        match std::thread::Builder::new()
            .name("algoritmoslab-vm".to_string())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                run_session_thread(
                    thread_run_id,
                    source,
                    options,
                    control_bg,
                    sink,
                    active_weak,
                );
            }) {
            Ok(handle) => {
                let mut guard = lock_unpoisoned(&self.active);
                if let Some(slot) = guard.as_mut() {
                    if Arc::ptr_eq(&slot.control, &control) {
                        slot.join_handle = Some(handle);
                    }
                }
                Ok(resolved_id)
            }
            Err(e) => {
                let mut guard = lock_unpoisoned(&self.active);
                if guard
                    .as_ref()
                    .is_some_and(|slot| Arc::ptr_eq(&slot.control, &control))
                {
                    *guard = None;
                }
                Err(format!("Falha ao iniciar thread de execução: {e}"))
            }
        }
    }

    pub fn send_input(&self, value: String) -> Result<(), String> {
        let Some(ctrl) = self.with_active_control() else {
            return Err("Nenhuma execução ativa aguardando entrada.".to_string());
        };
        ctrl.send_input(value)
    }

    pub fn debug_action(&self, action: DebugAction) -> Result<(), String> {
        let Some(ctrl) = self.with_active_control() else {
            return Err("Nenhuma execução ativa para depurar.".to_string());
        };
        ctrl.apply_action(action)
    }

    pub fn update_breakpoints(&self, lines: Vec<usize>) {
        if let Some(ctrl) = self.with_active_control() {
            ctrl.set_breakpoints(lines);
        }
    }

    pub fn stop(&self) {
        if let Some(ctrl) = self.with_active_control() {
            ctrl.cancel();
        }
    }

    pub fn stop_and_join(&self) {
        let _lifecycle = lock_unpoisoned(&self.lifecycle_lock);
        self.take_and_join_active();
    }
}

struct SessionFinalizer {
    control: Arc<SharedControl>,
    active: Weak<Mutex<Option<ActiveSession>>>,
}

impl Drop for SessionFinalizer {
    fn drop(&mut self) {
        self.control.mark_finished();
        if let Some(active_mutex) = self.active.upgrade() {
            let mut guard = lock_unpoisoned(&active_mutex);
            if guard
                .as_ref()
                .is_some_and(|slot| Arc::ptr_eq(&slot.control, &self.control))
            {
                *guard = None;
            }
        }
    }
}

fn emit_error(
    sink: &Arc<dyn EventSink>,
    run_id: &str,
    kind: ErrorKind,
    line: Option<usize>,
    column: Option<usize>,
    message: String,
    variables: Vec<VarSnapshot>,
) {
    sink.emit(
        run_id,
        SessionEvent::Error {
            kind,
            line,
            column,
            message,
            variables,
        },
    );
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "pânico interno desconhecido".to_string()
    }
}

fn run_session_thread(
    run_id: String,
    source: String,
    options: StartOptions,
    control: Arc<SharedControl>,
    sink: Arc<dyn EventSink>,
    active: Weak<Mutex<Option<ActiveSession>>>,
) {
    let _finalizer = SessionFinalizer {
        control: Arc::clone(&control),
        active,
    };

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_session_inner(&run_id, &source, options, &control, &sink)
    }));

    control.mark_finished();

    match outcome {
        Ok(terminal_event) => {
            sink.emit(&run_id, terminal_event);
        }
        Err(payload) => {
            let reason = panic_payload_message(payload.as_ref());
            emit_error(
                &sink,
                &run_id,
                ErrorKind::Execucao,
                None,
                None,
                format!("Erro interno inesperado durante a execução: {reason}"),
                Vec::new(),
            );
        }
    }
}

fn run_session_inner(
    run_id: &str,
    source: &str,
    options: StartOptions,
    control: &Arc<SharedControl>,
    sink: &Arc<dyn EventSink>,
) -> SessionEvent {
    let program = match compile(source) {
        Ok(p) => p,
        Err(err) => {
            return SessionEvent::Error {
                kind: err.kind,
                line: err.span.map(|s| s.line),
                column: err.span.map(|s| s.column),
                message: err.message,
                variables: Vec::new(),
            };
        }
    };

    let exec_opts = ExecOptions {
        max_call_depth: 500,
        strict_read: options.strict_read,
        rng_seed: None,
        sandbox_dir: options
            .sandbox_dir
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from),
    };

    let ctx = BridgeContext {
        run_id: run_id.to_string(),
        control: Arc::clone(control),
        sink: Arc::clone(sink),
        batcher: Rc::new(RefCell::new(OutputBatcher::new())),
    };

    let mut io_bridge = IoOnlyBridge { ctx: ctx.clone() };
    let mut dbg_bridge = DebugOnlyBridge {
        ctx: ctx.clone(),
        just_paused: false,
    };

    let mut vm = match Interpreter::new(&program, &mut io_bridge, &mut dbg_bridge, exec_opts) {
        Ok(v) => v,
        Err(err) => {
            return SessionEvent::Error {
                kind: err.kind,
                line: err.span.map(|s| s.line),
                column: err.span.map(|s| s.column),
                message: err.message,
                variables: Vec::new(),
            };
        }
    };

    let res = vm.run();
    let final_vars = vm.snapshot_vars();
    ctx.flush_output();

    match res {
        Ok(()) => SessionEvent::Finished {
            variables: final_vars,
        },
        Err(err) => SessionEvent::Error {
            kind: err.kind,
            line: err.span.map(|s| s.line),
            column: err.span.map(|s| s.column),
            message: err.message,
            variables: final_vars,
        },
    }
}

struct IoOnlyBridge {
    ctx: BridgeContext,
}

impl IoHandler for IoOnlyBridge {
    fn write(&mut self, text: &str) {
        self.ctx.push_output(text);
    }

    fn clear(&mut self) {
        self.ctx.clear_output();
    }

    fn write_error(&mut self, text: &str) {
        self.ctx.flush_output();
        self.ctx.emit(SessionEvent::ErrorNote {
            message: text.to_string(),
        });
    }

    fn echo_input(&mut self, text: &str) {
        self.ctx.flush_output();
        self.ctx.emit(SessionEvent::InputEcho {
            text: text.to_string(),
        });
    }

    fn set_console_color(
        &mut self,
        target: ConsoleColorTarget,
        color_hex: &str,
        _color_name: &str,
    ) {
        self.ctx.flush_output();
        let target_str = match target {
            ConsoleColorTarget::Foreground => "foreground",
            ConsoleColorTarget::Background => "background",
        };
        self.ctx.emit(SessionEvent::ConsoleColor {
            target: target_str.to_string(),
            color: color_hex.to_string(),
        });
    }

    fn read_input(
        &mut self,
        var_name: &str,
        expected_type: &str,
        line: usize,
    ) -> algoritmoslab_core::Result<String> {
        self.ctx.flush_output();
        self.ctx.check_cancel()?;

        {
            let mut st = lock_unpoisoned(&self.ctx.control.state);
            if self.ctx.control.cancel.load(Ordering::SeqCst) {
                return Err(AlgError::interrupted());
            }
            if let Some(v) = st.pending_input.pop_front() {
                return Ok(v);
            }
        }

        self.ctx.emit(SessionEvent::AwaitingInput {
            var_name: var_name.to_string(),
            expected_type: expected_type.to_string(),
            line,
        });

        let mut st = lock_unpoisoned(&self.ctx.control.state);
        loop {
            if self.ctx.control.cancel.load(Ordering::SeqCst) {
                return Err(AlgError::interrupted());
            }
            if let Some(v) = st.pending_input.pop_front() {
                return Ok(v);
            }
            st = wait_unpoisoned(&self.ctx.control.cvar, st);
        }
    }
}

struct DebugOnlyBridge {
    ctx: BridgeContext,
    just_paused: bool,
}

impl DebugOnlyBridge {
    fn pause_and_wait(
        &mut self,
        line: usize,
        call_depth: usize,
        snapshot: &mut dyn FnMut() -> DebugSnapshot,
    ) -> algoritmoslab_core::Result<()> {
        self.ctx.flush_output();
        let snap = snapshot();

        {
            let mut st = lock_unpoisoned(&self.ctx.control.state);
            if self.ctx.control.cancel.load(Ordering::SeqCst) {
                st.paused = false;
                st.paused_depth = None;
                return Err(AlgError::interrupted());
            }
            if !st.paused {
                return Ok(());
            }
        }

        self.ctx.emit(SessionEvent::Paused {
            line,
            call_depth,
            call_stack: snap.call_stack,
            variables: snap.variables,
        });

        let mut st = lock_unpoisoned(&self.ctx.control.state);
        while st.paused && !self.ctx.control.cancel.load(Ordering::SeqCst) {
            st = wait_unpoisoned(&self.ctx.control.cvar, st);
        }

        if self.ctx.control.cancel.load(Ordering::SeqCst) {
            st.paused = false;
            st.paused_depth = None;
            Err(AlgError::interrupted())
        } else {
            Ok(())
        }
    }
}

impl DebugController for DebugOnlyBridge {
    fn check_cancel(&self) -> algoritmoslab_core::Result<()> {
        self.ctx.check_cancel()
    }

    fn before_statement(
        &mut self,
        line: usize,
        call_depth: usize,
        snapshot: &mut dyn FnMut() -> DebugSnapshot,
    ) -> algoritmoslab_core::Result<()> {
        self.check_cancel()?;
        self.just_paused = false;

        {
            let mut st = lock_unpoisoned(&self.ctx.control.state);
            if self.ctx.control.cancel.load(Ordering::SeqCst) {
                return Err(AlgError::interrupted());
            }
            let should_pause = match st.mode {
                InternalMode::Run => st.breakpoints.contains(&line),
                InternalMode::PauseRequested | InternalMode::StepInto => true,
                InternalMode::StepOver { target_depth } => {
                    call_depth <= target_depth || st.breakpoints.contains(&line)
                }
            };
            if !should_pause {
                return Ok(());
            }
            st.paused = true;
            st.paused_depth = Some(call_depth);
        }

        self.just_paused = true;
        self.pause_and_wait(line, call_depth, snapshot)
    }

    fn pause_directive(
        &mut self,
        line: usize,
        call_depth: usize,
        snapshot: &mut dyn FnMut() -> DebugSnapshot,
    ) -> algoritmoslab_core::Result<()> {
        self.check_cancel()?;
        if self.just_paused {
            self.just_paused = false;
            return Ok(());
        }

        {
            let mut st = lock_unpoisoned(&self.ctx.control.state);
            if self.ctx.control.cancel.load(Ordering::SeqCst) {
                return Err(AlgError::interrupted());
            }
            st.paused = true;
            st.paused_depth = Some(call_depth);
        }

        self.pause_and_wait(line, call_depth, snapshot)
    }

    fn on_timer_trace(
        &mut self,
        line: usize,
        _call_depth: usize,
        snapshot: &mut dyn FnMut() -> DebugSnapshot,
    ) -> algoritmoslab_core::Result<()> {
        self.check_cancel()?;
        self.ctx.flush_output();
        let snap = snapshot();
        self.ctx.emit(SessionEvent::TraceLine {
            line,
            variables: snap.variables,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth_paths::{read_alg_file, write_alg_file, AuthorizedPaths, MAX_ALG_FILE_BYTES};
    use std::sync::mpsc;

    const TEST_TIMEOUT: Duration = Duration::from_secs(3);

    struct ChannelSink {
        tx: mpsc::Sender<SessionEventEnvelope>,
    }

    impl EventSink for ChannelSink {
        fn emit(&self, run_id: &str, event: SessionEvent) {
            let _ = self.tx.send(SessionEventEnvelope {
                run_id: run_id.to_string(),
                event,
            });
        }
    }

    struct TempTestDir {
        path: PathBuf,
    }

    impl TempTestDir {
        fn new(label: &str) -> Self {
            static DIR_SEQ: AtomicU64 = AtomicU64::new(1);
            let seq = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "algoritmoslab_test_{label}_{}_{}",
                std::process::id(),
                seq
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempTestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn start_test_session(
        mgr: &SessionManager,
        run_id: Option<&str>,
        source: &str,
        mode: ExecMode,
        breakpoints: Vec<usize>,
        options: StartOptions,
    ) -> (String, mpsc::Receiver<SessionEventEnvelope>) {
        let (tx, rx) = mpsc::channel();
        let sink = Arc::new(ChannelSink { tx });
        let id = mgr
            .start(
                run_id.map(str::to_string),
                source.to_string(),
                mode,
                breakpoints,
                options,
                sink,
            )
            .unwrap();
        (id, rx)
    }

    fn recv_event(rx: &mpsc::Receiver<SessionEventEnvelope>) -> SessionEvent {
        rx.recv_timeout(TEST_TIMEOUT).unwrap().event
    }

    fn expect_paused(
        rx: &mpsc::Receiver<SessionEventEnvelope>,
    ) -> (usize, usize, Vec<String>, Vec<VarSnapshot>) {
        while let Ok(env) = rx.recv_timeout(TEST_TIMEOUT) {
            match env.event {
                SessionEvent::Paused {
                    line,
                    call_depth,
                    call_stack,
                    variables,
                } => return (line, call_depth, call_stack, variables),
                SessionEvent::Output { .. } => {}
                other => panic!("esperava Paused, recebeu {other:?}"),
            }
        }
        panic!("timeout aguardando Paused");
    }

    fn collect_output_until_finished(rx: &mpsc::Receiver<SessionEventEnvelope>) -> String {
        let mut out = String::new();
        while let Ok(env) = rx.recv_timeout(TEST_TIMEOUT) {
            match env.event {
                SessionEvent::Output { text } => out.push_str(&text),
                SessionEvent::InputEcho { .. } => {}
                SessionEvent::Finished { .. } => return out,
                other => panic!("evento inesperado antes de Finished: {other:?}"),
            }
        }
        panic!("timeout aguardando Finished");
    }

    fn wait_for_error(rx: &mpsc::Receiver<SessionEventEnvelope>) -> (ErrorKind, String) {
        while let Ok(env) = rx.recv_timeout(TEST_TIMEOUT) {
            if let SessionEvent::Error { kind, message, .. } = env.event {
                return (kind, message);
            }
        }
        panic!("timeout aguardando SessionEvent::Error");
    }

    #[test]
    fn handles_interactive_input_and_completion() {
        let mgr = SessionManager::new();
        let src = r#"algoritmo "oi"
var
   nome: caractere
inicio
   escreva("Nome: ")
   leia(nome)
   escreval("Olá, ", nome)
fimalgoritmo
"#;

        let (run_id, rx) = start_test_session(
            &mgr,
            Some("run-1"),
            src,
            ExecMode::Run,
            Vec::new(),
            StartOptions::default(),
        );
        assert_eq!(run_id, "run-1");

        let mut saw_prompt = false;
        let mut saw_awaiting = false;
        let mut saw_echo = false;
        let mut finished_vars = None;

        while let Ok(env) = rx.recv_timeout(TEST_TIMEOUT) {
            assert_eq!(env.run_id, "run-1");
            match env.event {
                SessionEvent::Output { text } => {
                    if text.contains("Nome: ") {
                        saw_prompt = true;
                    }
                }
                SessionEvent::AwaitingInput { var_name, .. } => {
                    assert!(saw_prompt, "saída deve chegar antes de AwaitingInput");
                    assert_eq!(var_name, "nome");
                    saw_awaiting = true;
                    mgr.send_input("Maria".to_string()).unwrap();
                }
                SessionEvent::InputEcho { text } => {
                    assert_eq!(text, "Maria");
                    saw_echo = true;
                }
                SessionEvent::Finished { variables } => {
                    finished_vars = Some(variables);
                    break;
                }
                other => panic!("evento inesperado: {other:?}"),
            }
        }

        assert!(saw_awaiting);
        assert!(saw_echo);
        let vars = finished_vars.expect("deve finalizar com sucesso");
        assert_eq!(vars[0].name, "nome");
        assert_eq!(vars[0].value_text, "Maria");
    }

    #[test]
    fn steps_through_statements_and_cancels_infinite_loop() {
        let mgr = SessionManager::new();
        let src = r#"algoritmo "step"
var
   x: inteiro
inicio
   x <- 1
   x <- x + 2
   enquanto verdadeiro faca
      escreval("loop")
   fimenquanto
fimalgoritmo
"#;

        let (_, rx) = start_test_session(
            &mgr,
            None,
            src,
            ExecMode::StepInto,
            Vec::new(),
            StartOptions::default(),
        );

        let (line1, ..) = expect_paused(&rx);
        assert_eq!(line1, 5);
        mgr.debug_action(DebugAction::StepInto).unwrap();

        let (line2, _, call_stack, vars) = expect_paused(&rx);
        assert_eq!(line2, 6);
        assert_eq!(vars[0].value_text, "1");
        assert_eq!(call_stack, vec!["algoritmo \"step\""]);
        mgr.debug_action(DebugAction::Continue).unwrap();

        // Aguarda evento determinístico de execução dentro do loop infinito antes de parar
        match recv_event(&rx) {
            SessionEvent::Output { text } if !text.is_empty() => {}
            other => panic!("esperava saída do loop infinito, recebeu {other:?}"),
        }
        mgr.stop();

        let (kind, _) = wait_for_error(&rx);
        assert_eq!(kind, ErrorKind::Interrompido);
    }

    #[test]
    fn steps_over_subroutine_and_hits_breakpoint() {
        let mgr = SessionManager::new();
        let src = r#"algoritmo "step_over"
var
   total: inteiro

procedimento somar(d: inteiro)
inicio
   total <- total + d
fimprocedimento

inicio
   total <- 10
   somar(5)
   total <- total * 2
   escreval(total)
fimalgoritmo
"#;

        let (_, rx) = start_test_session(
            &mgr,
            None,
            src,
            ExecMode::StepInto,
            vec![14],
            StartOptions::default(),
        );

        let (line1, ..) = expect_paused(&rx);
        assert_eq!(line1, 11);
        mgr.debug_action(DebugAction::StepOver).unwrap();

        let (line2, ..) = expect_paused(&rx);
        assert_eq!(line2, 12);
        mgr.debug_action(DebugAction::StepOver).unwrap();

        let (line3, depth3, _, vars3) = expect_paused(&rx);
        assert_eq!(line3, 13);
        assert_eq!(depth3, 0);
        assert_eq!(vars3[0].value_text, "15");

        mgr.debug_action(DebugAction::Continue).unwrap();
        let (line4, _, _, vars4) = expect_paused(&rx);
        assert_eq!(line4, 14);
        assert_eq!(vars4[0].value_text, "30");

        mgr.debug_action(DebugAction::Continue).unwrap();
        let out = collect_output_until_finished(&rx);
        assert_eq!(out, "30\n");
    }

    #[test]
    fn steps_over_nested_subroutine_at_depth_gt_zero_and_rejects_unpaused_step() {
        let mgr = SessionManager::new();
        assert!(mgr.debug_action(DebugAction::StepOver).is_err());
        assert!(mgr.send_input("x".to_string()).is_err());

        let src = r#"algoritmo "nested_step_over"
var
   res: inteiro

procedimento interno()
inicio
   res <- res + 10
   res <- res + 5
fimprocedimento

procedimento externo()
inicio
   res <- 1
   interno()
   res <- res * 2
fimprocedimento

inicio
   res <- 0
   externo()
   escreval(res)
fimalgoritmo
"#;

        // Breakpoint na linha 13 (dentro de externo, call_depth == 1)
        let (_, rx) = start_test_session(
            &mgr,
            None,
            src,
            ExecMode::Run,
            vec![13],
            StartOptions::default(),
        );

        // Quando estiver rodando sem pausa, StepInto/StepOver devem ser rejeitados
        // (testamos também após pausar e continuar)
        let (line1, depth1, ..) = expect_paused(&rx);
        assert_eq!(line1, 13);
        assert_eq!(depth1, 1);

        // StepOver dentro de externo (depth == 1): vai para linha 14 (`interno()`)
        mgr.debug_action(DebugAction::StepOver).unwrap();
        let (line2, depth2, ..) = expect_paused(&rx);
        assert_eq!(line2, 14);
        assert_eq!(depth2, 1);

        // StepOver na chamada `interno()` estando em depth == 1 NÃO deve entrar em interno (depth 2)
        // e deve pausar na linha 15 (`res <- res * 2`) ainda em depth == 1!
        mgr.debug_action(DebugAction::StepOver).unwrap();
        let (line3, depth3, _, vars3) = expect_paused(&rx);
        assert_eq!(line3, 15);
        assert_eq!(depth3, 1);
        assert_eq!(vars3[0].value_text, "16");

        // StepOver na última linha de `externo` retorna ao chamador (depth 0, linha 21)
        mgr.debug_action(DebugAction::StepOver).unwrap();
        let (line4, depth4, _, vars4) = expect_paused(&rx);
        assert_eq!(line4, 21);
        assert_eq!(depth4, 0);
        assert_eq!(vars4[0].value_text, "32");

        mgr.debug_action(DebugAction::Continue).unwrap();
        // Enquanto não estiver pausado (após Continue / término), StepInto/StepOver falham
        let out = collect_output_until_finished(&rx);
        assert_eq!(out, "32\n");

        // Nova sessão com `leia` para testar rejeição de StepOver/StepInto enquanto não pausado
        let wait_src = r#"algoritmo "nao_pausado"
var
   x: inteiro
inicio
   leia(x)
   escreval(x)
fimalgoritmo
"#;
        let (_, rx_wait) = start_test_session(
            &mgr,
            None,
            wait_src,
            ExecMode::Run,
            Vec::new(),
            StartOptions::default(),
        );
        assert!(matches!(
            recv_event(&rx_wait),
            SessionEvent::AwaitingInput { .. }
        ));
        let err_over = mgr.debug_action(DebugAction::StepOver).unwrap_err();
        assert!(err_over.contains("pausada"));
        let err_into = mgr.debug_action(DebugAction::StepInto).unwrap_err();
        assert!(err_into.contains("pausada"));

        mgr.send_input("7".to_string()).unwrap();
        assert_eq!(collect_output_until_finished(&rx_wait), "7\n");
    }

    #[test]
    fn pauses_on_pausa_and_conditional_debug_directives() {
        let mgr = SessionManager::new();
        let src = r#"algoritmo "pausa_debug"
var
   x: inteiro
inicio
   x <- 1
   debug x = 5
   pausa
   x <- 5
   debug x = 5
   escreval(x)
fimalgoritmo
"#;

        let (_, rx) = start_test_session(
            &mgr,
            None,
            src,
            ExecMode::Run,
            Vec::new(),
            StartOptions::default(),
        );

        let (line1, _, _, vars1) = expect_paused(&rx);
        assert_eq!(line1, 7);
        assert_eq!(vars1[0].value_text, "1");
        mgr.debug_action(DebugAction::Continue).unwrap();

        let (line2, _, _, vars2) = expect_paused(&rx);
        assert_eq!(line2, 9);
        assert_eq!(vars2[0].value_text, "5");
        mgr.debug_action(DebugAction::Continue).unwrap();

        let out = collect_output_until_finished(&rx);
        assert_eq!(out, "5\n");
    }

    #[test]
    fn handles_eco_off_and_mudacor_events() {
        let mgr = SessionManager::new();
        let src = r#"algoritmo "eco_cores"
var
   a, b: inteiro
inicio
   mudacor("Amarelo", "FRENTE")
   mudacor("Azul", "FUNDOS")
   leia(a)
   eco off
   leia(b)
   eco on
   limpatela
   escreval(a + b)
fimalgoritmo
"#;

        let (_, rx) = start_test_session(
            &mgr,
            None,
            src,
            ExecMode::Run,
            Vec::new(),
            StartOptions::default(),
        );

        let mut colors = Vec::new();
        let mut echoes = Vec::new();
        let mut saw_clear = false;
        let mut output = String::new();
        let mut step_input = 0;

        while let Ok(env) = rx.recv_timeout(TEST_TIMEOUT) {
            match env.event {
                SessionEvent::ConsoleColor { target, color } => colors.push((target, color)),
                SessionEvent::AwaitingInput { var_name, .. } => {
                    step_input += 1;
                    if step_input == 1 {
                        assert_eq!(var_name, "a");
                        mgr.send_input("10".to_string()).unwrap();
                    } else {
                        assert_eq!(var_name, "b");
                        mgr.send_input("25".to_string()).unwrap();
                    }
                }
                SessionEvent::InputEcho { text } => echoes.push(text),
                SessionEvent::Clear => {
                    saw_clear = true;
                    output.clear();
                }
                SessionEvent::Output { text } => output.push_str(&text),
                SessionEvent::Finished { .. } => break,
                other => panic!("evento inesperado: {other:?}"),
            }
        }

        assert_eq!(
            colors,
            vec![
                ("foreground".to_string(), "#facc15".to_string()),
                ("background".to_string(), "#1d4ed8".to_string()),
            ]
        );
        assert_eq!(echoes, vec!["10".to_string()]);
        assert!(saw_clear);
        assert_eq!(output, "35\n");
    }

    #[test]
    fn emits_trace_line_cancels_timer_and_enforces_sandbox() {
        let mgr = SessionManager::new();
        let timer_src = r#"algoritmo "timer_lento"
var
   i: inteiro
inicio
   timer 500
   para i de 1 ate 100 faca
      escreval(i)
   fimpara
fimalgoritmo
"#;

        let (_, rx) = start_test_session(
            &mgr,
            None,
            timer_src,
            ExecMode::Run,
            Vec::new(),
            StartOptions::default(),
        );

        let mut saw_trace = false;
        while let Ok(env) = rx.recv_timeout(TEST_TIMEOUT) {
            if let SessionEvent::TraceLine { line, .. } = env.event {
                assert!(line >= 6);
                saw_trace = true;
                mgr.stop();
                break;
            }
        }
        assert!(saw_trace, "esperava TraceLine com timer ativo");
        let (kind, _) = wait_for_error(&rx);
        assert_eq!(kind, ErrorKind::Interrompido);

        let tmp = TempTestDir::new("sandbox");
        std::fs::write(tmp.path.join("entrada.txt"), "42\n").unwrap();

        let file_src = r#"algoritmo "arq_ok"
arquivo "entrada.txt"
var
   n: inteiro
inicio
   leia(n)
   escreval(n * 2)
fimalgoritmo
"#;

        let auth = AuthorizedPaths::new();
        let canonical_dir = auth.authorize_sandbox_dir(&tmp.path).unwrap();
        let validated_dir = auth
            .validate_sandbox_dir(&canonical_dir.to_string_lossy())
            .unwrap();

        let (_, rx2) = start_test_session(
            &mgr,
            None,
            file_src,
            ExecMode::Run,
            Vec::new(),
            StartOptions {
                strict_read: false,
                sandbox_dir: Some(validated_dir.to_string_lossy().to_string()),
            },
        );
        assert_eq!(collect_output_until_finished(&rx2), "84\n");

        // Sem sandbox_dir autorizado, o comando arquivo falha claramente
        let (_, rx3) = start_test_session(
            &mgr,
            None,
            file_src,
            ExecMode::Run,
            Vec::new(),
            StartOptions::default(),
        );
        let (_, err_msg) = wait_for_error(&rx3);
        assert!(err_msg.contains("arquivo"));
    }

    #[test]
    fn rapid_restarts_join_previous_vm_without_overlap() {
        let mgr = SessionManager::new();
        let tmp = TempTestDir::new("rapid_restart");
        std::fs::write(tmp.path.join("dados.txt"), "21\n").unwrap();
        let sandbox = tmp
            .path
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let loop_src = r#"algoritmo "loop_com_arquivo"
arquivo "dados.txt"
var
   n: inteiro
inicio
   leia(n)
   enquanto verdadeiro faca
      escreval(n)
   fimenquanto
fimalgoritmo
"#;

        for i in 0..8 {
            let run_id = format!("rapid-{i}");
            let (_, rx) = start_test_session(
                &mgr,
                Some(&run_id),
                loop_src,
                ExecMode::Run,
                Vec::new(),
                StartOptions {
                    strict_read: false,
                    sandbox_dir: Some(sandbox.clone()),
                },
            );
            // Aguarda pelo menos um Output desta execução antes de reiniciar imediatamente
            let first = recv_event(&rx);
            assert!(matches!(
                first,
                SessionEvent::Output { .. } | SessionEvent::InputEcho { .. }
            ));
        }

        let final_src = r#"algoritmo "final"
arquivo "dados.txt"
var
   n: inteiro
inicio
   leia(n)
   escreval(n * 2)
fimalgoritmo
"#;
        let (final_id, rx_final) = start_test_session(
            &mgr,
            Some("rapid-final"),
            final_src,
            ExecMode::Run,
            Vec::new(),
            StartOptions {
                strict_read: false,
                sandbox_dir: Some(sandbox),
            },
        );
        assert_eq!(final_id, "rapid-final");
        assert_eq!(collect_output_until_finished(&rx_final), "42\n");
        mgr.stop_and_join();
    }

    #[test]
    fn authorized_paths_and_file_io_enforce_allowlist_limits_and_symlinks() {
        let tmp = TempTestDir::new("auth_paths");
        let outside = TempTestDir::new("outside");

        let auth = AuthorizedPaths::new();
        let alg_path = tmp.path.join("teste.alg");
        let unauthorized_path = outside.path.join("secreto.alg");
        std::fs::write(
            &unauthorized_path,
            "algoritmo \"x\"\ninicio\nfimalgoritmo\n",
        )
        .unwrap();

        // Caminhos não autorizados devem falhar
        assert!(auth
            .validate_read_file(&unauthorized_path.to_string_lossy())
            .is_err());
        assert!(auth
            .validate_write_file(&alg_path.to_string_lossy())
            .is_err());
        assert!(auth
            .validate_sandbox_dir(&outside.path.to_string_lossy())
            .is_err());

        // Autorizar gravação em arquivo ainda inexistente autoriza write, read e sandbox_dir pai
        let canonical_save = auth.authorize_save_file(&alg_path).unwrap();
        let validated_write = auth
            .validate_write_file(&canonical_save.to_string_lossy())
            .unwrap();
        let sample = "algoritmo \"Ação\"\ninicio\n   escreval(\"Olá, coração!\")\nfimalgoritmo\n";
        write_alg_file(&validated_write, sample, SourceEncoding::Windows1252).unwrap();

        let validated_read = auth
            .validate_read_file(&canonical_save.to_string_lossy())
            .unwrap();
        let opened = read_alg_file(&validated_read).unwrap();
        assert_eq!(opened.encoding, SourceEncoding::Windows1252);
        assert_eq!(opened.text, sample);
        assert!(auth
            .validate_sandbox_dir(&tmp.path.to_string_lossy())
            .is_ok());

        // Autorizar abertura permite salvar de volta (Ctrl+S) e usar sandbox_dir pai
        let reopened_path = tmp.path.join("aberto.alg");
        std::fs::write(&reopened_path, "algoritmo \"utf8\"\ninicio\nfimalgoritmo\n").unwrap();
        let auth2 = AuthorizedPaths::new();
        let canonical_open = auth2.authorize_open_file(&reopened_path).unwrap();
        assert!(auth2
            .validate_read_file(&canonical_open.to_string_lossy())
            .is_ok());
        assert!(auth2
            .validate_write_file(&canonical_open.to_string_lossy())
            .is_ok());
        assert!(auth2
            .validate_sandbox_dir(&tmp.path.to_string_lossy())
            .is_ok());

        // Limite de 4 MiB na leitura e escrita
        let exact_limit_path = tmp.path.join("exact_4mib.alg");
        let exact_bytes = vec![b'a'; MAX_ALG_FILE_BYTES as usize];
        std::fs::write(&exact_limit_path, &exact_bytes).unwrap();
        assert!(read_alg_file(&exact_limit_path).is_ok());

        let over_limit_path = tmp.path.join("over_4mib.alg");
        let mut over_bytes = vec![b'a'; MAX_ALG_FILE_BYTES as usize];
        over_bytes.push(b'b');
        std::fs::write(&over_limit_path, &over_bytes).unwrap();
        assert!(read_alg_file(&over_limit_path).is_err());

        let over_text = "a".repeat(MAX_ALG_FILE_BYTES as usize + 1);
        assert!(write_alg_file(&canonical_save, &over_text, SourceEncoding::Utf8).is_err());

        // Expansão \n -> \r\n em Windows1252 excedendo 4 MiB deve ser rejeitada
        let crlf_expand = "\n".repeat((MAX_ALG_FILE_BYTES as usize / 2) + 1);
        assert!(
            write_alg_file(&canonical_save, &crlf_expand, SourceEncoding::Windows1252).is_err()
        );

        // Diretório não pode ser alvo de save/open
        assert!(auth.authorize_save_file(&tmp.path).is_err());
        assert!(auth.authorize_open_file(&tmp.path).is_err());
        assert!(write_alg_file(&tmp.path, "abc", SourceEncoding::Utf8).is_err());

        // Rejeição de symlinks em Unix
        #[cfg(unix)]
        {
            let link_file = tmp.path.join("atalho.alg");
            std::os::unix::fs::symlink(&unauthorized_path, &link_file).unwrap();
            assert!(auth.authorize_open_file(&link_file).is_err());
            assert!(auth.authorize_save_file(&link_file).is_err());
            assert!(auth
                .validate_read_file(&link_file.to_string_lossy())
                .is_err());
            assert!(read_alg_file(&link_file).is_err());
            assert!(write_alg_file(&link_file, "x", SourceEncoding::Utf8).is_err());

            // Symlink de diretório apontando para fora não autoriza diretório externo
            let link_dir = tmp.path.join("dir_externo");
            std::os::unix::fs::symlink(&outside.path, &link_dir).unwrap();
            assert!(auth
                .validate_sandbox_dir(&link_dir.to_string_lossy())
                .is_err());
        }
    }

    #[test]
    fn throttles_output_flood_and_recovers_from_sink_panic() {
        // 1) Flood de escreval é agrupado/limitado e cancelado imediatamente
        let mgr = SessionManager::new();
        let flood_src = r#"algoritmo "flood"
inicio
   enquanto verdadeiro faca
      escreval("linha-de-spam-para-testar-backpressure-ipc-seguro")
   fimenquanto
fimalgoritmo
"#;
        let (_, rx) = start_test_session(
            &mgr,
            None,
            flood_src,
            ExecMode::Run,
            Vec::new(),
            StartOptions::default(),
        );
        // Garante que chegou saída em lote e encerra rapidamente
        let first = recv_event(&rx);
        assert!(matches!(first, SessionEvent::Output { .. }));
        std::thread::sleep(Duration::from_millis(80));
        let mut events_before_stop = 1usize;
        while let Ok(env) = rx.try_recv() {
            if matches!(env.event, SessionEvent::Output { .. }) {
                events_before_stop += 1;
            }
        }
        assert!(
            events_before_stop < 200,
            "flood IPC não foi limitado: {events_before_stop} eventos em 80ms"
        );
        mgr.stop();
        let (kind, _) = wait_for_error(&rx);
        assert_eq!(kind, ErrorKind::Interrompido);

        // 2) Pânico inesperado dentro da thread da VM é capturado por catch_unwind
        struct PanicOnceSink {
            panicked: AtomicBool,
            tx: mpsc::Sender<SessionEventEnvelope>,
        }

        impl EventSink for PanicOnceSink {
            fn emit(&self, run_id: &str, event: SessionEvent) {
                if matches!(event, SessionEvent::Output { .. })
                    && !self.panicked.swap(true, Ordering::SeqCst)
                {
                    panic!("falha simulada no sink");
                }
                let _ = self.tx.send(SessionEventEnvelope {
                    run_id: run_id.to_string(),
                    event,
                });
            }
        }

        let (tx_panic, rx_panic) = mpsc::channel();
        let panic_sink = Arc::new(PanicOnceSink {
            panicked: AtomicBool::new(false),
            tx: tx_panic,
        });
        mgr.start(
            Some("panic-run".to_string()),
            "algoritmo \"p\"\ninicio\n   escreval(\"dispara\")\nfimalgoritmo\n".to_string(),
            ExecMode::Run,
            Vec::new(),
            StartOptions::default(),
            panic_sink,
        )
        .unwrap();

        let (err_kind, err_msg) = wait_for_error(&rx_panic);
        assert_eq!(err_kind, ErrorKind::Execucao);
        assert!(err_msg.contains("falha simulada no sink"));
        mgr.stop_and_join();

        // Após pânico, o SessionManager continua utilizável
        let (_, rx_ok) = start_test_session(
            &mgr,
            None,
            "algoritmo \"ok\"\ninicio\n   escreval(42)\nfimalgoritmo\n",
            ExecMode::Run,
            Vec::new(),
            StartOptions::default(),
        );
        assert_eq!(collect_output_until_finished(&rx_ok), "42\n");
    }

    #[test]
    fn starts_in_step_over_mode_supports_dynamic_pause_and_recovers_poisoned_lock() {
        // 1) ExecMode::StepOver no início pausa na primeira linha (call_depth == 0) e pula sub-rotina
        let mgr = SessionManager::new();
        let src = r#"algoritmo "inicio_step_over"
var
   v: inteiro

procedimento dobrar()
inicio
   v <- v * 2
fimprocedimento

inicio
   v <- 7
   dobrar()
   escreval(v)
fimalgoritmo
"#;

        let (_, rx) = start_test_session(
            &mgr,
            None,
            src,
            ExecMode::StepOver,
            Vec::new(),
            StartOptions::default(),
        );

        let (line1, depth1, ..) = expect_paused(&rx);
        assert_eq!(line1, 11);
        assert_eq!(depth1, 0);

        mgr.debug_action(DebugAction::StepOver).unwrap();
        let (line2, depth2, ..) = expect_paused(&rx);
        assert_eq!(line2, 12);
        assert_eq!(depth2, 0);

        mgr.debug_action(DebugAction::StepOver).unwrap();
        let (line3, depth3, _, vars3) = expect_paused(&rx);
        assert_eq!(line3, 13);
        assert_eq!(depth3, 0);
        assert_eq!(vars3[0].value_text, "14");

        mgr.debug_action(DebugAction::Continue).unwrap();
        assert_eq!(collect_output_until_finished(&rx), "14\n");

        // 2) DebugAction::Pause interrompe execução contínua e update_breakpoints funciona dinamicamente
        let pause_src = r#"algoritmo "pause_dinamico"
var
   i: inteiro
inicio
   i <- 0
   enquanto verdadeiro faca
      i <- i + 1
   fimenquanto
fimalgoritmo
"#;
        let (_, rx_pause) = start_test_session(
            &mgr,
            None,
            pause_src,
            ExecMode::Run,
            Vec::new(),
            StartOptions::default(),
        );
        mgr.debug_action(DebugAction::Pause).unwrap();
        let (paused_line, ..) = expect_paused(&rx_pause);
        assert!(paused_line >= 5);
        mgr.stop();
        let (kind, _) = wait_for_error(&rx_pause);
        assert_eq!(kind, ErrorKind::Interrompido);

        let bp_src = r#"algoritmo "bp_dinamico"
var
   x: inteiro
inicio
   x <- 1
   x <- x + 1
   x <- x + 1
   escreval(x)
fimalgoritmo
"#;
        let (_, rx_bp) = start_test_session(
            &mgr,
            None,
            bp_src,
            ExecMode::StepInto,
            Vec::new(),
            StartOptions::default(),
        );
        let (line_start, ..) = expect_paused(&rx_bp);
        assert_eq!(line_start, 5);
        mgr.update_breakpoints(vec![7]);
        mgr.debug_action(DebugAction::Continue).unwrap();
        let (bp_line, _, _, vars_bp) = expect_paused(&rx_bp);
        assert_eq!(bp_line, 7);
        assert_eq!(vars_bp[0].value_text, "2");
        mgr.debug_action(DebugAction::Continue).unwrap();
        assert_eq!(collect_output_until_finished(&rx_bp), "3\n");

        // 3) lock_unpoisoned recupera Mutex envenenado sem pânico
        let poisoned = Arc::new(Mutex::new(123usize));
        let poisoned_clone = Arc::clone(&poisoned);
        let _ = std::thread::spawn(move || {
            let mut guard = poisoned_clone.lock().unwrap();
            *guard = 456;
            panic!("envenena mutex");
        })
        .join();
        assert!(poisoned.is_poisoned());
        assert_eq!(*lock_unpoisoned(&poisoned), 456);
    }
}
