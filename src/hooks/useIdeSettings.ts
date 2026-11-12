import { useCallback, useEffect, useState } from "react";
import type { IdeSettings } from "../types/ide";

export const SETTINGS_STORAGE_KEY = "algoritmoslab.settings.v1";

export const DEFAULT_SETTINGS: IdeSettings = {
  theme: "light",
  examMode: false,
  strictRead: false,
  defaultSaveEncoding: "utf8",
  fontSize: 15,
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function validateSettings(raw: unknown): IdeSettings {
  if (!isRecord(raw)) {
    return DEFAULT_SETTINGS;
  }

  const theme = raw.theme === "dark" ? "dark" : "light";
  const examMode =
    typeof raw.examMode === "boolean"
      ? raw.examMode
      : DEFAULT_SETTINGS.examMode;
  const strictRead =
    typeof raw.strictRead === "boolean"
      ? raw.strictRead
      : DEFAULT_SETTINGS.strictRead;
  const defaultSaveEncoding =
    raw.defaultSaveEncoding === "windows1252" ? "windows1252" : "utf8";

  let fontSize = DEFAULT_SETTINGS.fontSize;
  if (typeof raw.fontSize === "number" && Number.isFinite(raw.fontSize)) {
    fontSize = Math.min(22, Math.max(12, Math.round(raw.fontSize)));
  }

  return {
    theme,
    examMode,
    strictRead,
    defaultSaveEncoding,
    fontSize,
  };
}

export function loadSettings(): IdeSettings {
  try {
    const raw = window.localStorage.getItem(SETTINGS_STORAGE_KEY);
    if (!raw) return DEFAULT_SETTINGS;
    const parsed: unknown = JSON.parse(raw);
    return validateSettings(parsed);
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export function useIdeSettings() {
  const [settings, setSettings] = useState<IdeSettings>(loadSettings);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", settings.theme);
    document.documentElement.style.colorScheme = settings.theme;
    try {
      window.localStorage.setItem(
        SETTINGS_STORAGE_KEY,
        JSON.stringify(settings)
      );
    } catch {
      // Ignora erro de quota
    }
  }, [settings]);

  const updateSettings = useCallback((patch: Partial<IdeSettings>) => {
    setSettings((prev) => validateSettings({ ...prev, ...patch }));
  }, []);

  return { settings, updateSettings };
}
