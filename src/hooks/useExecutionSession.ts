import { useCallback, useEffect, useRef, useState } from "react";
import type { IdeBridge } from "../services/tauriBridge";
import type {
  AwaitingInputState,
  ConsoleChunk,
  DiagnosticError,
  ErrorKind,
  ExecMode,
  ExecutionStatus,
  SessionEventEnvelope,
  VarSnapshot,
} from "../types/ide";
import { getParentDir } from "../utils/pathUtils";
import { usesArquivoCommand } from "../utils/algoritmosText";
import type { DocumentSnapshot } from "./useIdeDocument";

export const MAX_CONSOLE_CHUNKS = 800;
export const MAX_CONSOLE_CHARS = 250_000;
const OUTPUT_CHUNK_SPLIT_SIZE = 16_384;

export const ERROR_KIND_LABEL: Record<ErrorKind, string> = {
  lexico: "Erro Léxico",
  sintatico: "Erro de Sintaxe",
  semantico: "Erro Semântico",
  execucao: "Erro de Execução",
  interrompido: "Interrompido",
};

function makeRunId(): string {
  if (typeof globalThis.crypto?.randomUUID === "function") {
    return globalThis.crypto.randomUUID();
  }
  return `run-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

interface ConsoleBuffer {
  chunks: ConsoleChunk[];
  totalChars: number;
  truncated: boolean;
}

type ConsoleChunkInput =
  | { kind: "output"; text: string }
  | { kind: "input_echo"; text: string }
  | { kind: "note"; text: string }
  | { kind: "system"; text: string }
  | {
      kind: "error";
      text: string;
      line: number | null;
      column: number | null;
    };

function trimBufferToLimits(buf: ConsoleBuffer): void {
  while (
    buf.chunks.length > MAX_CONSOLE_CHUNKS ||
    (buf.totalChars > MAX_CONSOLE_CHARS && buf.chunks.length > 1)
  ) {
    const removed = buf.chunks.shift();
    if (!removed) break;
    buf.totalChars -= removed.text.length;
    buf.truncated = true;
  }

  if (buf.chunks.length === 1 && buf.totalChars > MAX_CONSOLE_CHARS) {
    const only = buf.chunks[0];
    const keepStart = only.text.length - MAX_CONSOLE_CHARS;
    const sliced = only.text.slice(keepStart);
    buf.chunks[0] = { ...only, text: sliced };
    buf.totalChars = sliced.length;
    buf.truncated = true;
  }
}

function scheduleFrame(cb: () => void): number {
  if (typeof requestAnimationFrame === "function") {
    return requestAnimationFrame(cb);
  }
  return window.setTimeout(cb, 16);
}

function cancelFrame(id: number): void {
  if (typeof cancelAnimationFrame === "function") {
    cancelAnimationFrame(id);
  } else {
    window.clearTimeout(id);
  }
}

interface UseExecutionSessionOptions {
  bridge: IdeBridge;
  strictRead: boolean;
  breakpointsRef: React.RefObject<number[]>;
  getDocumentSnapshot: () => DocumentSnapshot;
  onManualWorkingDirResolved: (dir: string) => void;
}

export function useExecutionSession({
  bridge,
  strictRead,
  breakpointsRef,
  getDocumentSnapshot,
  onManualWorkingDirResolved,
}: UseExecutionSessionOptions) {
  const [status, setStatus] = useState<ExecutionStatus>("idle");
  const [pausedLine, setPausedLine] = useState<number | null>(null);
  const [callStack, setCallStack] = useState<string[]>([]);
  const [variables, setVariables] = useState<VarSnapshot[]>([]);
  const [awaitingInput, setAwaitingInput] =
    useState<AwaitingInputState | null>(null);
  const [diagnostic, setDiagnostic] = useState<DiagnosticError | null>(null);
  const [consoleState, setConsoleState] = useState<{
    chunks: ConsoleChunk[];
    truncated: boolean;
  }>({ chunks: [], truncated: false });
  const [consoleFg, setConsoleFg] = useState<string | null>(null);
  const [consoleBg, setConsoleBg] = useState<string | null>(null);
  const [sessionResetSeq, setSessionResetSeq] = useState(0);

  const mountedRef = useRef(true);
  const activeRunIdRef = useRef<string | null>(null);
  const startingRef = useRef(false);
  const debugInFlightRef = useRef(false);
  const statusRef = useRef<ExecutionStatus>(status);
  const pausedLineRef = useRef<number | null>(pausedLine);
  const awaitingInputRef = useRef<AwaitingInputState | null>(awaitingInput);
  const strictReadRef = useRef(strictRead);
  strictReadRef.current = strictRead;

  const chunkIdRef = useRef(1);
  const consoleBufferRef = useRef<ConsoleBuffer>({
    chunks: [],
    totalChars: 0,
    truncated: false,
  });
  const pendingOutputRef = useRef("");
  const pendingTruncatedRef = useRef(false);
  const rafIdRef = useRef<number | null>(null);
  const listenerReadyPromiseRef = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (rafIdRef.current !== null) {
        cancelFrame(rafIdRef.current);
        rafIdRef.current = null;
      }
      pendingOutputRef.current = "";
      pendingTruncatedRef.current = false;
    };
  }, []);

  const getStatus = useCallback((): ExecutionStatus => statusRef.current, []);

  const setStatusSynced = useCallback((next: ExecutionStatus) => {
    statusRef.current = next;
    setStatus(next);
  }, []);

  const setPausedLineSynced = useCallback((next: number | null) => {
    pausedLineRef.current = next;
    setPausedLine(next);
  }, []);

  const setAwaitingInputSynced = useCallback(
    (next: AwaitingInputState | null) => {
      awaitingInputRef.current = next;
      setAwaitingInput(next);
    },
    []
  );

  const commitConsoleSnapshot = useCallback(() => {
    if (!mountedRef.current) return;
    setConsoleState({
      chunks: [...consoleBufferRef.current.chunks],
      truncated: consoleBufferRef.current.truncated,
    });
  }, []);

  const pushOutputIntoBuffer = useCallback((text: string) => {
    if (!text) return;
    const buf = consoleBufferRef.current;
    let remaining = text;
    if (remaining.length > MAX_CONSOLE_CHARS) {
      remaining = remaining.slice(remaining.length - MAX_CONSOLE_CHARS);
      buf.truncated = true;
    }

    if (remaining.length > 0 && buf.chunks.length > 0) {
      const lastIdx = buf.chunks.length - 1;
      const last = buf.chunks[lastIdx];
      if (last.kind === "output" && last.text.length < OUTPUT_CHUNK_SPLIT_SIZE) {
        const space = OUTPUT_CHUNK_SPLIT_SIZE - last.text.length;
        const head = remaining.slice(0, space);
        buf.chunks[lastIdx] = { ...last, text: last.text + head };
        buf.totalChars += head.length;
        remaining = remaining.slice(space);
      }
    }

    while (remaining.length > 0) {
      const piece = remaining.slice(0, OUTPUT_CHUNK_SPLIT_SIZE);
      remaining = remaining.slice(OUTPUT_CHUNK_SPLIT_SIZE);
      const id = chunkIdRef.current++;
      buf.chunks.push({ id, kind: "output", text: piece });
      buf.totalChars += piece.length;
    }

    trimBufferToLimits(buf);
  }, []);

  const pushChunkIntoBuffer = useCallback((chunk: ConsoleChunkInput) => {
    const id = chunkIdRef.current++;
    const fullChunk: ConsoleChunk = { ...chunk, id };
    const buf = consoleBufferRef.current;
    buf.chunks.push(fullChunk);
    buf.totalChars += fullChunk.text.length;
    trimBufferToLimits(buf);
  }, []);

  const cancelScheduledOutputFlush = useCallback(() => {
    if (rafIdRef.current !== null) {
      cancelFrame(rafIdRef.current);
      rafIdRef.current = null;
    }
  }, []);

  const flushPendingOutput = useCallback(
    (shouldCommit: boolean) => {
      cancelScheduledOutputFlush();
      let changed = false;
      if (pendingTruncatedRef.current) {
        consoleBufferRef.current.truncated = true;
        pendingTruncatedRef.current = false;
        changed = true;
      }
      if (pendingOutputRef.current.length > 0) {
        const text = pendingOutputRef.current;
        pendingOutputRef.current = "";
        pushOutputIntoBuffer(text);
        changed = true;
      }
      if (shouldCommit && changed) {
        commitConsoleSnapshot();
      }
    },
    [cancelScheduledOutputFlush, commitConsoleSnapshot, pushOutputIntoBuffer]
  );

  const appendChunk = useCallback(
    (chunk: ConsoleChunkInput) => {
      flushPendingOutput(false);
      pushChunkIntoBuffer(chunk);
      commitConsoleSnapshot();
    },
    [commitConsoleSnapshot, flushPendingOutput, pushChunkIntoBuffer]
  );

  const clearConsole = useCallback(() => {
    cancelScheduledOutputFlush();
    pendingOutputRef.current = "";
    pendingTruncatedRef.current = false;
    consoleBufferRef.current = {
      chunks: [],
      totalChars: 0,
      truncated: false,
    };
    commitConsoleSnapshot();
  }, [cancelScheduledOutputFlush, commitConsoleSnapshot]);

  const queueOutput = useCallback(
    (text: string) => {
      pendingOutputRef.current += text;
      if (pendingOutputRef.current.length > MAX_CONSOLE_CHARS * 2) {
        pendingOutputRef.current = pendingOutputRef.current.slice(
          pendingOutputRef.current.length - MAX_CONSOLE_CHARS
        );
        pendingTruncatedRef.current = true;
      }
      if (rafIdRef.current === null) {
        rafIdRef.current = scheduleFrame(() => {
          rafIdRef.current = null;
          if (!mountedRef.current) return;
          flushPendingOutput(true);
        });
      }
    },
    [flushPendingOutput]
  );

  const handleEnvelopeRef = useRef<(envelope: SessionEventEnvelope) => void>(
    () => {}
  );
  handleEnvelopeRef.current = (envelope: SessionEventEnvelope) => {
    if (envelope.run_id !== activeRunIdRef.current) {
      return;
    }
    const ev = envelope.event;
    switch (ev.type) {
      case "output": {
        queueOutput(ev.text);
        break;
      }
      case "input_echo": {
        appendChunk({ kind: "input_echo", text: ev.text });
        break;
      }
      case "console_color": {
        flushPendingOutput(true);
        const nextColor = ev.color ? ev.color : null;
        if (ev.target === "foreground") {
          setConsoleFg(nextColor);
        } else if (ev.target === "background") {
          setConsoleBg(nextColor);
        }
        break;
      }
      case "clear": {
        clearConsole();
        break;
      }
      case "error_note": {
        appendChunk({ kind: "note", text: ev.message });
        break;
      }
      case "trace_line": {
        flushPendingOutput(true);
        setPausedLineSynced(ev.line);
        setVariables(ev.variables);
        break;
      }
      case "awaiting_input": {
        flushPendingOutput(true);
        setStatusSynced("awaiting_input");
        setPausedLineSynced(ev.line);
        setAwaitingInputSynced({
          varName: ev.var_name,
          expectedType: ev.expected_type,
          line: ev.line,
        });
        break;
      }
      case "paused": {
        flushPendingOutput(true);
        setStatusSynced("paused");
        setPausedLineSynced(ev.line);
        setCallStack(ev.call_stack);
        setVariables(ev.variables);
        setAwaitingInputSynced(null);
        break;
      }
      case "finished": {
        flushPendingOutput(false);
        setStatusSynced("finished");
        setPausedLineSynced(null);
        setCallStack([]);
        setAwaitingInputSynced(null);
        setVariables(ev.variables);
        pushChunkIntoBuffer({
          kind: "system",
          text: "Execução concluída com sucesso.",
        });
        commitConsoleSnapshot();
        break;
      }
      case "error": {
        flushPendingOutput(false);
        setAwaitingInputSynced(null);
        setPausedLineSynced(null);
        setCallStack([]);
        if (ev.variables.length > 0) {
          setVariables(ev.variables);
        }
        if (ev.kind === "interrompido") {
          setStatusSynced("stopped");
          pushChunkIntoBuffer({ kind: "system", text: ev.message });
        } else {
          setStatusSynced("error");
          setDiagnostic({
            kind: ev.kind,
            line: ev.line,
            column: ev.column,
            message: ev.message,
          });
          pushChunkIntoBuffer({
            kind: "error",
            text: `[${ERROR_KIND_LABEL[ev.kind]}] ${ev.message}`,
            line: ev.line,
            column: ev.column,
          });
        }
        commitConsoleSnapshot();
        break;
      }
    }
  };

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let active = true;

    const ready = bridge
      .listenSessionEvents((envelope) => {
        if (active) {
          handleEnvelopeRef.current(envelope);
        }
      })
      .then((fn) => {
        if (active) {
          unlisten = fn;
        } else {
          fn();
        }
      })
      .catch((err) => {
        console.error("Falha ao registrar listener de sessão:", err);
      });

    listenerReadyPromiseRef.current = ready;

    return () => {
      active = false;
      unlisten?.();
    };
  }, [bridge]);

  const resetExecutionUi = useCallback(() => {
    setPausedLineSynced(null);
    setCallStack([]);
    setAwaitingInputSynced(null);
    setDiagnostic(null);
    setVariables([]);
    clearConsole();
    setConsoleFg(null);
    setConsoleBg(null);
    setStatusSynced("idle");
    setSessionResetSeq((s) => s + 1);
  }, [clearConsole, setAwaitingInputSynced, setPausedLineSynced, setStatusSynced]);

  const abortAndResetSession = useCallback(async () => {
    activeRunIdRef.current = null;
    await bridge.stopExecution().catch(() => {});
    resetExecutionUi();
  }, [bridge, resetExecutionUi]);

  const startSession = useCallback(
    async (mode: ExecMode) => {
      if (startingRef.current) return;
      const currentStatus = statusRef.current;
      if (currentStatus === "running" || currentStatus === "awaiting_input") {
        return;
      }

      startingRef.current = true;
      try {
        const snapshot = getDocumentSnapshot();
        let resolvedSandboxDir =
          snapshot.manualWorkingDir ??
          (snapshot.filePath ? getParentDir(snapshot.filePath) : null);

        if (!resolvedSandboxDir && usesArquivoCommand(snapshot.source)) {
          const pickedDir = await bridge.pickWorkingDir();
          if (!mountedRef.current) return;

          const afterPickerSnapshot = getDocumentSnapshot();
          if (afterPickerSnapshot.docRevision !== snapshot.docRevision) {
            appendChunk({
              kind: "note",
              text: "Execução cancelada: o algoritmo foi modificado durante a seleção da pasta autorizada.",
            });
            return;
          }

          if (!pickedDir) {
            appendChunk({
              kind: "note",
              text: 'Execução cancelada: salve o algoritmo (.alg) ou selecione uma pasta autorizada para usar o comando "arquivo".',
            });
            return;
          }
          resolvedSandboxDir = pickedDir;
          onManualWorkingDirResolved(pickedDir);
        }

        await listenerReadyPromiseRef.current;
        if (!mountedRef.current) return;

        const preStartSnapshot = getDocumentSnapshot();
        if (preStartSnapshot.docRevision !== snapshot.docRevision) {
          return;
        }

        const runId = makeRunId();
        activeRunIdRef.current = runId;
        clearConsole();
        setConsoleFg(null);
        setConsoleBg(null);
        setDiagnostic(null);
        setPausedLineSynced(null);
        setCallStack([]);
        setAwaitingInputSynced(null);
        setVariables([]);
        setSessionResetSeq((s) => s + 1);
        setStatusSynced("running");

        try {
          await bridge.startExecution({
            runId,
            source: snapshot.source,
            mode,
            breakpoints: breakpointsRef.current,
            options: {
              strict_read: strictReadRef.current,
              sandbox_dir: resolvedSandboxDir,
            },
          });
        } catch (err) {
          if (activeRunIdRef.current === runId) {
            setStatusSynced("error");
            appendChunk({
              kind: "error",
              text: String(err),
              line: null,
              column: null,
            });
          }
        }
      } finally {
        startingRef.current = false;
      }
    },
    [
      appendChunk,
      breakpointsRef,
      bridge,
      clearConsole,
      getDocumentSnapshot,
      onManualWorkingDirResolved,
      setAwaitingInputSynced,
      setPausedLineSynced,
      setStatusSynced,
    ]
  );

  const dispatchDebugAction = useCallback(
    async (action: "continue" | "step_into" | "step_over") => {
      if (debugInFlightRef.current || statusRef.current !== "paused") return;
      debugInFlightRef.current = true;

      const capturedRunId = activeRunIdRef.current;
      const prevStatus = statusRef.current;
      const prevPausedLine = pausedLineRef.current;

      setStatusSynced("running");
      if (action === "continue") {
        setPausedLineSynced(null);
      }

      try {
        await bridge.debugAction(action);
      } catch (err) {
        if (
          activeRunIdRef.current === capturedRunId &&
          getStatus() === "running"
        ) {
          setStatusSynced(prevStatus);
          setPausedLineSynced(prevPausedLine);
        }
        appendChunk({
          kind: "error",
          text: String(err),
          line: null,
          column: null,
        });
      } finally {
        debugInFlightRef.current = false;
      }
    },
    [appendChunk, bridge, getStatus, setPausedLineSynced, setStatusSynced]
  );

  const handleRun = useCallback(async () => {
    if (statusRef.current === "paused") {
      await dispatchDebugAction("continue");
      return;
    }
    if (
      statusRef.current === "running" ||
      statusRef.current === "awaiting_input"
    ) {
      return;
    }
    await startSession("run");
  }, [dispatchDebugAction, startSession]);

  const handleStepInto = useCallback(async () => {
    if (statusRef.current === "paused") {
      await dispatchDebugAction("step_into");
      return;
    }
    if (
      statusRef.current === "running" ||
      statusRef.current === "awaiting_input"
    ) {
      return;
    }
    await startSession("step_into");
  }, [dispatchDebugAction, startSession]);

  const handleStepOver = useCallback(async () => {
    if (statusRef.current === "paused") {
      await dispatchDebugAction("step_over");
      return;
    }
    if (
      statusRef.current === "running" ||
      statusRef.current === "awaiting_input"
    ) {
      return;
    }
    await startSession("step_over");
  }, [dispatchDebugAction, startSession]);

  const handlePause = useCallback(async () => {
    if (statusRef.current !== "running") return;
    try {
      await bridge.debugAction("pause");
    } catch (err) {
      appendChunk({
        kind: "error",
        text: String(err),
        line: null,
        column: null,
      });
    }
  }, [appendChunk, bridge]);

  const handleStop = useCallback(async () => {
    try {
      await bridge.stopExecution();
    } catch (err) {
      appendChunk({
        kind: "error",
        text: String(err),
        line: null,
        column: null,
      });
    }
  }, [appendChunk, bridge]);

  const handleSubmitInput = useCallback(
    async (value: string) => {
      if (statusRef.current !== "awaiting_input") return;
      const capturedRunId = activeRunIdRef.current;
      const prevAwaiting = awaitingInputRef.current;
      const prevPausedLine = pausedLineRef.current;

      setAwaitingInputSynced(null);
      setPausedLineSynced(null);
      setStatusSynced("running");

      try {
        await bridge.sendInput(value);
      } catch (err) {
        if (
          activeRunIdRef.current === capturedRunId &&
          getStatus() === "running"
        ) {
          setAwaitingInputSynced(prevAwaiting);
          setPausedLineSynced(prevPausedLine);
          setStatusSynced("awaiting_input");
        }
        appendChunk({
          kind: "error",
          text: String(err),
          line: null,
          column: null,
        });
      }
    },
    [
      appendChunk,
      bridge,
      getStatus,
      setAwaitingInputSynced,
      setPausedLineSynced,
      setStatusSynced,
    ]
  );

  const syncBreakpointsWithSession = useCallback(
    (lines: number[]) => {
      const s = statusRef.current;
      if (s === "running" || s === "paused" || s === "awaiting_input") {
        void bridge.updateBreakpoints(lines).catch(() => {});
      }
    },
    [bridge]
  );

  const appendErrorChunk = useCallback(
    (message: string) => {
      appendChunk({
        kind: "error",
        text: message,
        line: null,
        column: null,
      });
    },
    [appendChunk]
  );

  const clearDiagnostic = useCallback(() => {
    setDiagnostic(null);
  }, []);

  return {
    status,
    pausedLine,
    callStack,
    variables,
    awaitingInput,
    diagnostic,
    chunks: consoleState.chunks,
    truncated: consoleState.truncated,
    consoleFg,
    consoleBg,
    sessionResetSeq,
    handleRun,
    handleStepInto,
    handleStepOver,
    handlePause,
    handleStop,
    handleSubmitInput,
    clearConsole,
    clearDiagnostic,
    appendErrorChunk,
    abortAndResetSession,
    syncBreakpointsWithSession,
  };
}
