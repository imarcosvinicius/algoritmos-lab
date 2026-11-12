export type SourceEncoding = "utf8" | "windows1252";

export type ExecMode = "run" | "step_into" | "step_over";

export type DebugAction = "step_into" | "step_over" | "continue" | "pause";

export type ErrorKind =
  | "lexico"
  | "sintatico"
  | "semantico"
  | "execucao"
  | "interrompido";

export interface OpenedFile {
  path: string;
  name: string;
  text: string;
  encoding: SourceEncoding;
}

export interface VarArrayItem {
  index_label: string;
  value_text: string;
}

export interface VarSnapshot {
  name: string;
  type_label: string;
  value_text: string;
  scope: string;
  items?: VarArrayItem[];
  total_items?: number;
}

export type SessionEvent =
  | { type: "output"; text: string }
  | { type: "input_echo"; text: string }
  | { type: "console_color"; target: "foreground" | "background"; color: string }
  | { type: "clear" }
  | { type: "error_note"; message: string }
  | { type: "trace_line"; line: number; variables: VarSnapshot[] }
  | {
      type: "awaiting_input";
      var_name: string;
      expected_type: string;
      line: number;
    }
  | {
      type: "paused";
      line: number;
      call_depth: number;
      call_stack: string[];
      variables: VarSnapshot[];
    }
  | {
      type: "finished";
      variables: VarSnapshot[];
    }
  | {
      type: "error";
      kind: ErrorKind;
      line: number | null;
      column: number | null;
      message: string;
      variables: VarSnapshot[];
    };

export interface SessionEventEnvelope {
  run_id: string;
  event: SessionEvent;
}

export interface StartOptions {
  strict_read: boolean;
  sandbox_dir?: string | null;
}

export type ExecutionStatus =
  | "idle"
  | "running"
  | "paused"
  | "awaiting_input"
  | "finished"
  | "error"
  | "stopped";

export type ThemeMode = "light" | "dark";

export interface IdeSettings {
  theme: ThemeMode;
  examMode: boolean;
  strictRead: boolean;
  defaultSaveEncoding: SourceEncoding;
  fontSize: number;
}

export interface DiagnosticError {
  kind: ErrorKind;
  line: number | null;
  column: number | null;
  message: string;
}

export interface AwaitingInputState {
  varName: string;
  expectedType: string;
  line: number;
}

export type ConsoleChunk =
  | { id: number; kind: "output"; text: string }
  | { id: number; kind: "input_echo"; text: string }
  | { id: number; kind: "note"; text: string }
  | { id: number; kind: "system"; text: string }
  | {
      id: number;
      kind: "error";
      text: string;
      line: number | null;
      column: number | null;
    };

export interface AlgorithmExample {
  id: string;
  title: string;
  level: "Iniciante" | "Estruturas" | "Sub-rotinas";
  description: string;
  tags: string[];
  source: string;
}
