import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { confirm } from "@tauri-apps/plugin-dialog";
import type {
  DebugAction,
  ExecMode,
  OpenedFile,
  SessionEventEnvelope,
  SourceEncoding,
  StartOptions,
} from "../types/ide";

export interface IdeBridge {
  isDesktop: () => boolean;
  getCoreVersion: () => Promise<string>;
  pickOpenAlgPath: () => Promise<string | null>;
  pickSaveAlgPath: (defaultName?: string) => Promise<string | null>;
  pickWorkingDir: () => Promise<string | null>;
  openAlgFile: (path: string) => Promise<OpenedFile>;
  saveAlgFile: (
    path: string,
    text: string,
    encoding: SourceEncoding
  ) => Promise<void>;
  confirmAction: (message: string, title?: string) => Promise<boolean>;
  setWindowTitle: (title: string) => Promise<void>;
  onWindowCloseRequested: (
    handler: () => Promise<boolean>
  ) => Promise<UnlistenFn>;
  startExecution: (params: {
    runId: string;
    source: string;
    mode: ExecMode;
    breakpoints: number[];
    options: StartOptions;
  }) => Promise<string>;
  sendInput: (value: string) => Promise<void>;
  debugAction: (action: DebugAction) => Promise<void>;
  updateBreakpoints: (lines: number[]) => Promise<void>;
  stopExecution: () => Promise<void>;
  listenSessionEvents: (
    handler: (envelope: SessionEventEnvelope) => void
  ) => Promise<UnlistenFn>;
}

export const tauriIdeBridge: IdeBridge = {
  isDesktop: () => isTauri(),

  getCoreVersion: async () => {
    if (!isTauri()) return "0.2.0-preview";
    return invoke<string>("get_core_version");
  },

  pickOpenAlgPath: async () => {
    if (!isTauri()) return null;
    return invoke<string | null>("pick_open_alg_path");
  },

  pickSaveAlgPath: async (defaultName?: string) => {
    if (!isTauri()) return null;
    return invoke<string | null>("pick_save_alg_path", {
      defaultName: defaultName ?? null,
    });
  },

  pickWorkingDir: async () => {
    if (!isTauri()) return null;
    return invoke<string | null>("pick_working_dir");
  },

  openAlgFile: async (path: string) => {
    if (!isTauri()) {
      throw new Error(
        "A abertura de arquivos nativos está disponível apenas no aplicativo desktop Tauri."
      );
    }
    return invoke<OpenedFile>("open_alg_file", { path });
  },

  saveAlgFile: async (
    path: string,
    text: string,
    encoding: SourceEncoding
  ) => {
    if (!isTauri()) {
      throw new Error(
        "A gravação de arquivos nativos está disponível apenas no aplicativo desktop Tauri."
      );
    }
    await invoke("save_alg_file", { path, text, encoding });
  },

  confirmAction: async (message: string, title = "AlgoritmosLab") => {
    if (isTauri()) {
      return confirm(message, { title, kind: "warning" });
    }
    return window.confirm(message);
  },

  setWindowTitle: async (title: string) => {
    if (isTauri()) {
      try {
        await getCurrentWindow().setTitle(title);
      } catch {
        // Ignora silenciosamente se janela não suportar
      }
    } else {
      document.title = title;
    }
  },

  onWindowCloseRequested: async (handler) => {
    if (!isTauri()) {
      return () => {};
    }
    return getCurrentWindow().onCloseRequested(async (event) => {
      const allowClose = await handler();
      if (!allowClose) {
        event.preventDefault();
      }
    });
  },

  startExecution: async ({ runId, source, mode, breakpoints, options }) => {
    if (!isTauri()) {
      throw new Error(
        "O motor de execução nativo está disponível apenas no aplicativo desktop Tauri."
      );
    }
    return invoke<string>("start_execution", {
      runId,
      source,
      mode,
      breakpoints,
      options,
    });
  },

  sendInput: async (value: string) => {
    if (!isTauri()) return;
    await invoke("send_input", { value });
  },

  debugAction: async (action: DebugAction) => {
    if (!isTauri()) return;
    await invoke("debug_action", { action });
  },

  updateBreakpoints: async (lines: number[]) => {
    if (!isTauri()) return;
    await invoke("update_breakpoints", { lines });
  },

  stopExecution: async () => {
    if (!isTauri()) return;
    await invoke("stop_execution");
  },

  listenSessionEvents: async (handler) => {
    if (!isTauri()) {
      return () => {};
    }
    return listen<SessionEventEnvelope>("alg-session-event", (ev) => {
      handler(ev.payload);
    });
  },
};
