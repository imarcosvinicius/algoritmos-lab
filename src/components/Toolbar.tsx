import { memo } from "react";
import {
  BookOpen,
  FileCode2,
  FilePlus2,
  FolderOpen,
  GraduationCap,
  Moon,
  Pause,
  Play,
  Redo2,
  Save,
  Settings,
  Square,
  StepForward,
  Sun,
} from "lucide-react";
import type { ExecutionStatus, SourceEncoding, ThemeMode } from "../types/ide";

interface ToolbarProps {
  fileName: string;
  isDirty: boolean;
  encoding: SourceEncoding;
  status: ExecutionStatus;
  examMode: boolean;
  theme: ThemeMode;
  onNew: () => void;
  onOpen: () => void;
  onSave: () => void;
  onSaveAsWindows1252: () => void;
  onRun: () => void;
  onStepInto: () => void;
  onStepOver: () => void;
  onPause: () => void;
  onStop: () => void;
  onOpenExamples: () => void;
  onOpenSettings: () => void;
  onToggleExamMode: () => void;
  onToggleTheme: () => void;
}

export const Toolbar = memo(function Toolbar({
  fileName,
  isDirty,
  encoding,
  status,
  examMode,
  theme,
  onNew,
  onOpen,
  onSave,
  onSaveAsWindows1252,
  onRun,
  onStepInto,
  onStepOver,
  onPause,
  onStop,
  onOpenExamples,
  onOpenSettings,
  onToggleExamMode,
  onToggleTheme,
}: ToolbarProps) {
  const isRunning = status === "running";
  const isAwaitingInput = status === "awaiting_input";
  const isPaused = status === "paused";
  const isActive = isRunning || isAwaitingInput || isPaused;

  return (
    <header className="ide-toolbar">
      <div className="toolbar-left">
        <div
          className="brand-badge"
          title="AlgoritmosLab — IDE de Portugol Estruturado"
        >
          <FileCode2 size={18} className="brand-icon" aria-hidden="true" />
          <span className="brand-name">AlgoritmosLab</span>
        </div>

        <div className="toolbar-divider" aria-hidden="true" />

        <div className="toolbar-group" role="group" aria-label="Arquivo">
          <button
            type="button"
            className="tb-btn"
            onClick={onNew}
            title="Novo algoritmo (Ctrl+N)"
          >
            <FilePlus2 size={15} aria-hidden="true" />
            <span>Novo</span>
          </button>
          <button
            type="button"
            className="tb-btn"
            onClick={onOpen}
            title="Abrir arquivo .alg (Ctrl+O)"
          >
            <FolderOpen size={15} aria-hidden="true" />
            <span>Abrir</span>
          </button>
          <button
            type="button"
            className="tb-btn"
            onClick={onSave}
            title="Salvar arquivo (Ctrl+S)"
          >
            <Save size={15} aria-hidden="true" />
            <span>Salvar</span>
          </button>
          <button
            type="button"
            className="tb-btn tb-btn-subtle"
            onClick={onSaveAsWindows1252}
            title="Salvar cópia em Windows-1252 (compatível com ambientes legados em Windows)"
          >
            <span>Win-1252</span>
          </button>
        </div>

        <div
          className="file-pill"
          title={`${fileName} — Encoding: ${
            encoding === "utf8" ? "UTF-8" : "Windows-1252"
          }`}
        >
          <span className="file-pill-name">
            <span className="file-pill-filename">{fileName}</span>
            {isDirty && (
              <span className="dirty-dot" title="Alterações não salvas">
                *
              </span>
            )}
          </span>
          <span className="file-pill-enc">
            {encoding === "utf8" ? "UTF-8" : "Win-1252"}
          </span>
        </div>
      </div>

      <div
        className="toolbar-center"
        role="group"
        aria-label="Execução e depuração"
      >
        {!isActive && (
          <>
            <button
              type="button"
              className="tb-btn tb-btn-run"
              onClick={onRun}
              title="Executar algoritmo (F9)"
            >
              <Play size={15} aria-hidden="true" />
              <span>Executar (F9)</span>
            </button>
            <button
              type="button"
              className="tb-btn"
              onClick={onStepInto}
              title="Iniciar depuração passo a passo (F8)"
            >
              <StepForward size={15} aria-hidden="true" />
              <span>Passo (F8)</span>
            </button>
          </>
        )}

        {isPaused && (
          <>
            <button
              type="button"
              className="tb-btn tb-btn-run"
              onClick={onRun}
              title="Continuar execução (F9)"
            >
              <Play size={15} aria-hidden="true" />
              <span>Continuar (F9)</span>
            </button>
            <button
              type="button"
              className="tb-btn"
              onClick={onStepInto}
              title="Próximo passo / Entrar em sub-rotina (F8)"
            >
              <StepForward size={15} aria-hidden="true" />
              <span>Entrar (F8)</span>
            </button>
            <button
              type="button"
              className="tb-btn"
              onClick={onStepOver}
              title="Pular sub-rotina (Shift+F8)"
            >
              <Redo2 size={15} aria-hidden="true" />
              <span>Pular</span>
            </button>
          </>
        )}

        {isRunning && (
          <button
            type="button"
            className="tb-btn"
            onClick={onPause}
            title="Pausar no próximo comando"
          >
            <Pause size={15} aria-hidden="true" />
            <span>Pausar</span>
          </button>
        )}

        {isAwaitingInput && (
          <span className="toolbar-input-badge" role="status">
            Aguardando leia()
          </span>
        )}

        {isActive && (
          <button
            type="button"
            className="tb-btn tb-btn-stop"
            onClick={onStop}
            title="Parar execução imediatamente"
          >
            <Square size={14} aria-hidden="true" />
            <span>Parar</span>
          </button>
        )}
      </div>

      <div
        className="toolbar-right"
        role="group"
        aria-label="Ferramentas e preferências"
      >
        <button
          type="button"
          className="tb-btn"
          onClick={onOpenExamples}
          title="Abrir galeria de exemplos didáticos"
        >
          <BookOpen size={15} aria-hidden="true" />
          <span>Exemplos</span>
        </button>

        <button
          type="button"
          className={`tb-btn ${examMode ? "tb-btn-active-warn" : ""}`}
          onClick={onToggleExamMode}
          aria-pressed={examMode}
          title={
            examMode
              ? "Modo Prova ativo: autocomplete e fechamento automático desativados"
              : "Ativar Modo Prova (desativa autocomplete e auto-fechamento)"
          }
        >
          <GraduationCap size={15} aria-hidden="true" />
          <span>{examMode ? "Modo Prova: ON" : "Modo Prova"}</span>
        </button>

        <button
          type="button"
          className="tb-icon-btn"
          onClick={onOpenSettings}
          aria-label="Configurações"
          title="Configurações"
        >
          <Settings size={16} aria-hidden="true" />
        </button>

        <button
          type="button"
          className="tb-icon-btn"
          onClick={onToggleTheme}
          aria-label={
            theme === "dark" ? "Ativar tema claro" : "Ativar tema escuro"
          }
          title={
            theme === "dark" ? "Mudar para tema claro" : "Mudar para tema escuro"
          }
        >
          {theme === "dark" ? (
            <Sun size={16} aria-hidden="true" />
          ) : (
            <Moon size={16} aria-hidden="true" />
          )}
        </button>
      </div>
    </header>
  );
});
