import { FormEvent, memo, useEffect, useRef, useState } from "react";
import {
  AlertCircle,
  CornerDownLeft,
  Folder,
  Terminal,
  Trash2,
} from "lucide-react";
import type {
  AwaitingInputState,
  ConsoleChunk,
  DiagnosticError,
  ExecutionStatus,
  ThemeMode,
} from "../types/ide";
import { resolveConsoleColors } from "../utils/colorUtils";
import { getFileName } from "../utils/pathUtils";

const MIN_CONSOLE_HEIGHT = 130;
const MAX_CONSOLE_HEIGHT = 520;
const BOTTOM_STICK_THRESHOLD_PX = 36;

interface ConsolePanelProps {
  status: ExecutionStatus;
  pausedLine: number | null;
  chunks: ConsoleChunk[];
  truncated?: boolean;
  awaitingInput: AwaitingInputState | null;
  diagnostic: DiagnosticError | null;
  consoleFg: string | null;
  consoleBg: string | null;
  workingDir: string | null;
  theme?: ThemeMode;
  height: number;
  onResizeHeight: (nextHeight: number) => void;
  onClearConsole: () => void;
  onSubmitInput: (value: string) => void;
  onJumpToLocation: (line: number, column?: number | null) => void;
  onSelectWorkingDir: () => void;
}

const STATUS_LABEL: Record<ExecutionStatus, string> = {
  idle: "Pronto",
  running: "Executando...",
  paused: "Pausado",
  awaiting_input: "Aguardando entrada",
  finished: "Concluído",
  error: "Erro",
  stopped: "Interrompido",
};

export const ConsolePanel = memo(function ConsolePanel({
  status,
  pausedLine,
  chunks,
  truncated = false,
  awaitingInput,
  diagnostic,
  consoleFg,
  consoleBg,
  workingDir,
  theme = "light",
  height,
  onResizeHeight,
  onClearConsole,
  onSubmitInput,
  onJumpToLocation,
  onSelectWorkingDir,
}: ConsolePanelProps) {
  const [inputValue, setInputValue] = useState("");
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const outputEndRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const stickToBottomRef = useRef(true);
  const dragCleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    return () => {
      dragCleanupRef.current?.();
      dragCleanupRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (chunks.length === 0) {
      stickToBottomRef.current = true;
    }
  }, [chunks.length]);

  useEffect(() => {
    if (stickToBottomRef.current || awaitingInput) {
      outputEndRef.current?.scrollIntoView?.({ block: "end" });
    }
  }, [chunks, awaitingInput]);

  useEffect(() => {
    if (awaitingInput) {
      setInputValue("");
      inputRef.current?.focus();
    }
  }, [awaitingInput]);

  const handleBodyScroll = () => {
    const el = bodyRef.current;
    if (!el) return;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    stickToBottomRef.current = distanceFromBottom <= BOTTOM_STICK_THRESHOLD_PX;
  };

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!awaitingInput) return;
    stickToBottomRef.current = true;
    onSubmitInput(inputValue);
    setInputValue("");
  };

  const handleClear = () => {
    stickToBottomRef.current = true;
    onClearConsole();
  };

  const handleDragStart = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    dragCleanupRef.current?.();

    const startY = e.clientY;
    const startHeight = height;

    const onMove = (ev: MouseEvent) => {
      const delta = startY - ev.clientY;
      const clamped = Math.max(
        MIN_CONSOLE_HEIGHT,
        Math.min(MAX_CONSOLE_HEIGHT, startHeight + delta)
      );
      onResizeHeight(clamped);
    };

    const cleanup = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", cleanup);
      window.removeEventListener("blur", cleanup);
      dragCleanupRef.current = null;
    };

    dragCleanupRef.current = cleanup;
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", cleanup);
    window.addEventListener("blur", cleanup);
  };

  const handleResizerKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 40 : 16;
    if (e.key === "ArrowUp") {
      e.preventDefault();
      onResizeHeight(Math.min(MAX_CONSOLE_HEIGHT, height + step));
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      onResizeHeight(Math.max(MIN_CONSOLE_HEIGHT, height - step));
    } else if (e.key === "Home") {
      e.preventDefault();
      onResizeHeight(MAX_CONSOLE_HEIGHT);
    } else if (e.key === "End") {
      e.preventDefault();
      onResizeHeight(MIN_CONSOLE_HEIGHT);
    }
  };

  const statusText =
    status === "paused" && pausedLine !== null
      ? `Pausado na linha ${pausedLine}`
      : STATUS_LABEL[status];

  const resolvedColors = resolveConsoleColors({
    fg: consoleFg,
    bg: consoleBg,
    theme,
  });
  const hasCustomColor = Boolean(resolvedColors.bg || resolvedColors.fg);
  const consoleBodyStyle: React.CSSProperties = {
    ...(resolvedColors.bg ? { backgroundColor: resolvedColors.bg } : {}),
    ...(resolvedColors.fg ? { color: resolvedColors.fg } : {}),
  };

  const diagLine = diagnostic?.line ?? null;
  const diagColumn = diagnostic?.column ?? null;
  const workingDirLabel = workingDir
    ? getFileName(workingDir, workingDir)
    : null;

  return (
    <section
      className="console-panel"
      style={{ height: `${height}px` }}
      aria-label="Console de execução"
    >
      <div
        className="console-resizer"
        onMouseDown={handleDragStart}
        onKeyDown={handleResizerKeyDown}
        role="separator"
        tabIndex={0}
        aria-orientation="horizontal"
        aria-label="Redimensionar altura do console"
        aria-valuemin={MIN_CONSOLE_HEIGHT}
        aria-valuemax={MAX_CONSOLE_HEIGHT}
        aria-valuenow={height}
        title="Arraste ou use as setas para redimensionar o console"
      />

      <header className="panel-header console-header">
        <div className="console-header-left">
          <span className="panel-title">
            <Terminal size={14} aria-hidden="true" />
            Console Integrado
          </span>
          <span
            className={`status-pill status-${status}`}
            role="status"
            aria-live="polite"
          >
            {statusText}
          </span>
        </div>

        <div className="console-header-actions">
          <button
            type="button"
            className={`btn-ghost-sm console-dir-btn ${
              workingDir ? "active" : ""
            }`}
            onClick={onSelectWorkingDir}
            title={
              workingDir
                ? `Pasta autorizada para 'arquivo': ${workingDir}`
                : "Selecionar pasta autorizada para o comando 'arquivo'"
            }
          >
            <Folder size={13} aria-hidden="true" />
            <span>
              {workingDirLabel ? `Pasta: ${workingDirLabel}` : "Pasta arquivo"}
            </span>
          </button>
          {diagnostic && diagLine !== null && (
            <button
              type="button"
              className="btn-link-error"
              onClick={() => onJumpToLocation(diagLine, diagColumn)}
            >
              <AlertCircle size={13} aria-hidden="true" />
              Ir para linha {diagLine}
              {diagColumn ? `:${diagColumn}` : ""}
            </button>
          )}
          <button
            type="button"
            className="btn-ghost-sm"
            onClick={handleClear}
            title="Limpar saída do console"
          >
            <Trash2 size={13} aria-hidden="true" />
            Limpar
          </button>
        </div>
      </header>

      {truncated && (
        <div className="console-truncated-banner" role="status">
          Saída anterior truncada para manter o console responsivo (exibindo os
          blocos mais recentes).
        </div>
      )}

      <div
        ref={bodyRef}
        className={`console-body ${
          hasCustomColor ? "console-body-colored" : ""
        }`}
        style={consoleBodyStyle}
        data-testid="console-output"
        onScroll={handleBodyScroll}
      >
        {chunks.length === 0 && !awaitingInput ? (
          <div className="console-placeholder">
            A saída de <code>escreva</code>/<code>escreval</code> e solicitações
            de <code>leia</code> aparecerão aqui. Pressione <kbd>F9</kbd> para
            executar.
          </div>
        ) : (
          <div className="console-stream">
            {chunks.map((chunk) => {
              if (chunk.kind === "output") {
                return (
                  <span key={chunk.id} className="console-out-text">
                    {chunk.text}
                  </span>
                );
              }
              if (chunk.kind === "input_echo") {
                return (
                  <span key={chunk.id} className="console-in-echo">
                    {chunk.text}
                    {"\n"}
                  </span>
                );
              }
              if (chunk.kind === "note") {
                return (
                  <div key={chunk.id} className="console-note-line">
                    {chunk.text}
                  </div>
                );
              }
              if (chunk.kind === "system") {
                return (
                  <div key={chunk.id} className="console-system-line">
                    {chunk.text}
                  </div>
                );
              }
              const chunkLine = chunk.line;
              return (
                <div key={chunk.id} className="console-error-block">
                  <span>{chunk.text}</span>
                  {chunkLine !== null && (
                    <button
                      type="button"
                      className="console-error-jump"
                      onClick={() => onJumpToLocation(chunkLine, chunk.column)}
                    >
                      Linha {chunkLine}
                      {chunk.column ? `:${chunk.column}` : ""}
                    </button>
                  )}
                </div>
              );
            })}
            <div ref={outputEndRef} />
          </div>
        )}
      </div>

      {awaitingInput && (
        <form className="console-input-bar" onSubmit={handleSubmit}>
          <label htmlFor="console-leia-input" className="console-input-prompt">
            <span>
              Entrada para <strong>{awaitingInput.varName}</strong>{" "}
              <small>({awaitingInput.expectedType})</small>:
            </span>
          </label>
          <input
            id="console-leia-input"
            ref={inputRef}
            type="text"
            className="console-input-field"
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            placeholder={`Digite um valor ${awaitingInput.expectedType} e pressione Enter`}
          />
          <button type="submit" className="btn-primary-sm">
            <CornerDownLeft size={13} aria-hidden="true" />
            Enviar
          </button>
        </form>
      )}
    </section>
  );
});
