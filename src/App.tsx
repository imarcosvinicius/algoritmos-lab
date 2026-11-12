import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { AlertTriangle, X } from "lucide-react";
import { CodeEditor, type JumpTarget } from "./components/CodeEditor";
import { ConsolePanel } from "./components/ConsolePanel";
import {
  StatusBar,
  type CursorPosition,
  type CursorSubscribeFn,
} from "./components/StatusBar";
import { Toolbar } from "./components/Toolbar";
import { VariableWatchPanel } from "./components/VariableWatchPanel";
import {
  ERROR_KIND_LABEL,
  useExecutionSession,
} from "./hooks/useExecutionSession";
import { useIdeDocument } from "./hooks/useIdeDocument";
import { useIdeSettings } from "./hooks/useIdeSettings";
import { tauriIdeBridge, type IdeBridge } from "./services/tauriBridge";
import type { AlgorithmExample } from "./types/ide";

const LazyExamplesModal = lazy(() =>
  import("./components/ExamplesModal").then((m) => ({
    default: m.ExamplesModal,
  }))
);

const LazySettingsModal = lazy(() =>
  import("./components/SettingsModal").then((m) => ({
    default: m.SettingsModal,
  }))
);

export interface AppProps {
  bridge?: IdeBridge;
}

export function App({ bridge = tauriIdeBridge }: AppProps) {
  const { settings, updateSettings } = useIdeSettings();
  const [coreVersion, setCoreVersion] = useState("0.2.0");

  const [breakpointLines, setBreakpointLines] = useState<number[]>([]);
  const breakpointsRef = useRef<number[]>(breakpointLines);
  breakpointsRef.current = breakpointLines;

  const [jumpTarget, setJumpTarget] = useState<JumpTarget | null>(null);
  const jumpSeqRef = useRef(1);

  const [consoleHeight, setConsoleHeight] = useState(210);
  const [examplesOpen, setExamplesOpen] = useState(false);
  const [examplesMounted, setExamplesMounted] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsMounted, setSettingsMounted] = useState(false);

  const abortSessionRef = useRef<() => Promise<void>>(async () => {});
  const appendErrorRef = useRef<(message: string) => void>(() => {});

  const handleBeforeReplaceDocument = useCallback(async () => {
    await abortSessionRef.current();
  }, []);

  const handleFileError = useCallback((message: string) => {
    appendErrorRef.current(message);
  }, []);

  const doc = useIdeDocument({
    bridge,
    defaultSaveEncoding: settings.defaultSaveEncoding,
    onBeforeReplaceDocument: handleBeforeReplaceDocument,
    onFileError: handleFileError,
  });

  const session = useExecutionSession({
    bridge,
    strictRead: settings.strictRead,
    breakpointsRef,
    getDocumentSnapshot: doc.getSnapshot,
    onManualWorkingDirResolved: doc.setManualWorkingDir,
  });

  abortSessionRef.current = session.abortAndResetSession;
  appendErrorRef.current = session.appendErrorChunk;

  // Consulta versão do core
  useEffect(() => {
    let mounted = true;
    bridge
      .getCoreVersion()
      .then((ver) => {
        if (mounted) setCoreVersion(ver);
      })
      .catch(() => {});
    return () => {
      mounted = false;
    };
  }, [bridge]);

  // Proteção contra fechamento da janela com alterações não salvas
  const closingRef = useRef(false);
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let mounted = true;

    bridge
      .onWindowCloseRequested(async () => {
        if (closingRef.current) return false;
        closingRef.current = true;
        try {
          if (doc.isDirtyRef.current) {
            const confirmed = await bridge.confirmAction(
              "Há alterações não salvas no algoritmo atual. Deseja sair mesmo assim?",
              "Alterações não salvas"
            );
            if (!confirmed) return false;
            doc.dismissRecoveryDraft();
          }
          await bridge.stopExecution().catch(() => {});
          return true;
        } finally {
          closingRef.current = false;
        }
      })
      .then((fn) => {
        if (mounted) unlisten = fn;
        else fn();
      })
      .catch((err) => {
        console.error("Falha ao registrar proteção de fechamento:", err);
      });

    return () => {
      mounted = false;
      unlisten?.();
    };
  }, [bridge, doc.dismissRecoveryDraft, doc.isDirtyRef]);

  // Isolamento de posição do cursor para evitar re-renderizações em massa
  const cursorListenersRef = useRef<Set<(pos: CursorPosition) => void>>(
    new Set()
  );
  const lastCursorPosRef = useRef<CursorPosition>({ line: 1, column: 1 });

  const subscribeCursor = useCallback<CursorSubscribeFn>((listener) => {
    cursorListenersRef.current.add(listener);
    listener(lastCursorPosRef.current);
    return () => {
      cursorListenersRef.current.delete(listener);
    };
  }, []);

  const handleCursorChange = useCallback((line: number, column: number) => {
    const next = { line, column };
    lastCursorPosRef.current = next;
    cursorListenersRef.current.forEach((listener) => listener(next));
  }, []);

  const handleEditorChange = useCallback(
    (next: string) => {
      doc.setSource(next);
      session.clearDiagnostic();
    },
    [doc.setSource, session.clearDiagnostic]
  );

  const handleBreakpointsChange = useCallback(
    (lines: number[]) => {
      setBreakpointLines(lines);
      session.syncBreakpointsWithSession(lines);
    },
    [session.syncBreakpointsWithSession]
  );

  const handleJumpToLocation = useCallback(
    (line: number, column?: number | null) => {
      setJumpTarget({ line, column, seq: jumpSeqRef.current++ });
    },
    []
  );

  const handleOpenExamples = useCallback(() => {
    setExamplesMounted(true);
    setExamplesOpen(true);
  }, []);

  const handleCloseExamples = useCallback(() => {
    setExamplesOpen(false);
  }, []);

  const handleOpenSettings = useCallback(() => {
    setSettingsMounted(true);
    setSettingsOpen(true);
  }, []);

  const handleCloseSettings = useCallback(() => {
    setSettingsOpen(false);
  }, []);

  const handleSelectExample = useCallback(
    async (example: AlgorithmExample) => {
      setExamplesOpen(false);
      await doc.loadExample(example);
    },
    [doc.loadExample]
  );

  const handleToggleExamMode = useCallback(() => {
    updateSettings({ examMode: !settings.examMode });
  }, [settings.examMode, updateSettings]);

  const handleToggleTheme = useCallback(() => {
    updateSettings({ theme: settings.theme === "dark" ? "light" : "dark" });
  }, [settings.theme, updateSettings]);

  const onNewAction = useCallback(() => {
    void doc.handleNew();
  }, [doc.handleNew]);

  const onOpenAction = useCallback(() => {
    void doc.handleOpen();
  }, [doc.handleOpen]);

  const onSaveAction = useCallback(() => {
    void doc.handleSave();
  }, [doc.handleSave]);

  const onSaveWin1252Action = useCallback(() => {
    void doc.handleSaveAsWindows1252();
  }, [doc.handleSaveAsWindows1252]);

  const onRunAction = useCallback(() => {
    void session.handleRun();
  }, [session.handleRun]);

  const onStepIntoAction = useCallback(() => {
    void session.handleStepInto();
  }, [session.handleStepInto]);

  const onStepOverAction = useCallback(() => {
    void session.handleStepOver();
  }, [session.handleStepOver]);

  const onPauseAction = useCallback(() => {
    void session.handlePause();
  }, [session.handlePause]);

  const onStopAction = useCallback(() => {
    void session.handleStop();
  }, [session.handleStop]);

  const onSubmitInputAction = useCallback(
    (val: string) => {
      void session.handleSubmitInput(val);
    },
    [session.handleSubmitInput]
  );

  const onSelectWorkingDirAction = useCallback(() => {
    void doc.handleSelectWorkingDir();
  }, [doc.handleSelectWorkingDir]);

  // Atalhos globais de teclado (F8, Shift+F8, F9, Ctrl+S/O/N)
  const modalOpenRef = useRef(false);
  modalOpenRef.current = examplesOpen || settingsOpen;

  const shortcutsRef = useRef({
    handleRun: session.handleRun,
    handleStepInto: session.handleStepInto,
    handleStepOver: session.handleStepOver,
    handleSave: doc.handleSave,
    handleOpen: doc.handleOpen,
    handleNew: doc.handleNew,
  });
  shortcutsRef.current = {
    handleRun: session.handleRun,
    handleStepInto: session.handleStepInto,
    handleStepOver: session.handleStepOver,
    handleSave: doc.handleSave,
    handleOpen: doc.handleOpen,
    handleNew: doc.handleNew,
  };

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (modalOpenRef.current || e.altKey || e.isComposing) return;

      if (e.key === "F9") {
        e.preventDefault();
        void shortcutsRef.current.handleRun();
      } else if (e.key === "F8" && e.shiftKey) {
        e.preventDefault();
        void shortcutsRef.current.handleStepOver();
      } else if (e.key === "F8") {
        e.preventDefault();
        void shortcutsRef.current.handleStepInto();
      } else if ((e.ctrlKey || e.metaKey) && !e.shiftKey) {
        const target = e.target;
        const isFormField =
          target instanceof HTMLElement &&
          (target.tagName === "INPUT" ||
            target.tagName === "TEXTAREA" ||
            target.tagName === "SELECT");
        if (isFormField) return;

        const lower = e.key.toLowerCase();
        if (lower === "s") {
          e.preventDefault();
          void shortcutsRef.current.handleSave();
        } else if (lower === "o") {
          e.preventDefault();
          void shortcutsRef.current.handleOpen();
        } else if (lower === "n") {
          e.preventDefault();
          void shortcutsRef.current.handleNew();
        }
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const isSessionActive =
    session.status === "running" ||
    session.status === "paused" ||
    session.status === "awaiting_input";

  const watchResetKey = useMemo(
    () => `${doc.docResetKey}:${session.sessionResetSeq}`,
    [doc.docResetKey, session.sessionResetSeq]
  );

  return (
    <div className="ide-shell">
      <Toolbar
        fileName={doc.fileName}
        isDirty={doc.isDirty}
        encoding={doc.encoding}
        status={session.status}
        examMode={settings.examMode}
        theme={settings.theme}
        onNew={onNewAction}
        onOpen={onOpenAction}
        onSave={onSaveAction}
        onSaveAsWindows1252={onSaveWin1252Action}
        onRun={onRunAction}
        onStepInto={onStepIntoAction}
        onStepOver={onStepOverAction}
        onPause={onPauseAction}
        onStop={onStopAction}
        onOpenExamples={handleOpenExamples}
        onOpenSettings={handleOpenSettings}
        onToggleExamMode={handleToggleExamMode}
        onToggleTheme={handleToggleTheme}
      />

      {doc.pendingRecoveryDraft !== null && (
        <div
          className="recovery-banner"
          role="region"
          aria-label="Recuperação de rascunho"
        >
          <div className="recovery-banner-message">
            <AlertTriangle size={15} aria-hidden="true" />
            <span>Rascunho não salvo encontrado após interrupção.</span>
          </div>
          <div className="recovery-banner-actions">
            <button
              type="button"
              className="btn-primary-sm"
              onClick={() => void doc.restoreRecoveryDraft()}
            >
              Restaurar no editor
            </button>
            <button
              type="button"
              className="btn-ghost-sm"
              onClick={doc.dismissRecoveryDraft}
            >
              Descartar rascunho
            </button>
          </div>
        </div>
      )}

      <main className="ide-workspace">
        <div className="ide-top-split">
          <div className="editor-pane">
            {session.diagnostic &&
              (() => {
                const diag = session.diagnostic;
                const diagLine = diag.line;
                const diagnosticContent = (
                  <>
                    <AlertTriangle size={15} aria-hidden="true" />
                    <span>
                      <strong>{ERROR_KIND_LABEL[diag.kind]}</strong>
                      {diagLine !== null
                        ? ` (linha ${diagLine}${
                            diag.column ? `:${diag.column}` : ""
                          })`
                        : ""}
                      : {diag.message}
                    </span>
                  </>
                );

                return (
                  <div className="diagnostic-banner" role="alert">
                    {diagLine !== null ? (
                      <button
                        type="button"
                        className="diagnostic-jump"
                        onClick={() =>
                          handleJumpToLocation(diagLine, diag.column)
                        }
                      >
                        {diagnosticContent}
                      </button>
                    ) : (
                      <div className="diagnostic-summary">
                        {diagnosticContent}
                      </div>
                    )}
                    <button
                      type="button"
                      className="diagnostic-dismiss"
                      onClick={session.clearDiagnostic}
                      aria-label="Fechar aviso de erro"
                    >
                      <X size={14} aria-hidden="true" />
                    </button>
                  </div>
                );
              })()}

            <CodeEditor
              value={doc.source}
              docResetKey={doc.docResetKey}
              onChange={handleEditorChange}
              onBreakpointsChange={handleBreakpointsChange}
              onCursorChange={handleCursorChange}
              pausedLine={session.pausedLine}
              errorLine={session.diagnostic?.line ?? null}
              jumpTarget={jumpTarget}
              examMode={settings.examMode}
              theme={settings.theme}
              fontSize={settings.fontSize}
              readOnly={isSessionActive}
            />
          </div>

          <VariableWatchPanel
            variables={session.variables}
            callStack={session.callStack}
            isPaused={session.status === "paused"}
            resetKey={watchResetKey}
          />
        </div>

        <ConsolePanel
          status={session.status}
          pausedLine={session.pausedLine}
          chunks={session.chunks}
          truncated={session.truncated}
          awaitingInput={session.awaitingInput}
          diagnostic={session.diagnostic}
          consoleFg={session.consoleFg}
          consoleBg={session.consoleBg}
          workingDir={doc.authorizedSandboxDir}
          theme={settings.theme}
          height={consoleHeight}
          onResizeHeight={setConsoleHeight}
          onClearConsole={session.clearConsole}
          onSubmitInput={onSubmitInputAction}
          onJumpToLocation={handleJumpToLocation}
          onSelectWorkingDir={onSelectWorkingDirAction}
        />
      </main>

      <StatusBar
        coreVersion={coreVersion}
        subscribeCursor={subscribeCursor}
        breakpointCount={breakpointLines.length}
        examMode={settings.examMode}
        encoding={doc.encoding}
      />

      {examplesMounted && (
        <Suspense fallback={null}>
          <LazyExamplesModal
            open={examplesOpen}
            onClose={handleCloseExamples}
            onSelectExample={(ex) => void handleSelectExample(ex)}
          />
        </Suspense>
      )}

      {settingsMounted && (
        <Suspense fallback={null}>
          <LazySettingsModal
            open={settingsOpen}
            settings={settings}
            onClose={handleCloseSettings}
            onUpdate={updateSettings}
          />
        </Suspense>
      )}
    </div>
  );
}

export default App;
