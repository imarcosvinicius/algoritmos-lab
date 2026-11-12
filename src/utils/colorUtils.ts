import type { ThemeMode } from "../types/ide";

const HEX_COLOR_RE = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/;

const DEFAULT_CONSOLE_BG: Record<ThemeMode, string> = {
  light: "#fbfcfd",
  dark: "#10151e",
};

const SAFE_LIGHT_TEXT = "#f8fafc";
const SAFE_DARK_TEXT = "#0f172a";
const SAFE_DARK_BG = "#0f172a";
const SAFE_LIGHT_BG = "#f8fafc";

export function isValidHexColor(
  value: string | null | undefined
): value is string {
  return typeof value === "string" && HEX_COLOR_RE.test(value.trim());
}

function parseHexRgb(hex: string): [number, number, number] | null {
  if (!isValidHexColor(hex)) return null;
  const clean = hex.trim().slice(1);
  const full =
    clean.length === 3
      ? clean
          .split("")
          .map((c) => c + c)
          .join("")
      : clean;
  const r = Number.parseInt(full.slice(0, 2), 16);
  const g = Number.parseInt(full.slice(2, 4), 16);
  const b = Number.parseInt(full.slice(4, 6), 16);
  if (Number.isNaN(r) || Number.isNaN(g) || Number.isNaN(b)) return null;
  return [r, g, b];
}

function linearizeChannel(channel255: number): number {
  const c = channel255 / 255;
  return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
}

export function getRelativeLuminance(hex: string): number | null {
  const rgb = parseHexRgb(hex);
  if (!rgb) return null;
  const [r, g, b] = rgb;
  return (
    0.2126 * linearizeChannel(r) +
    0.7152 * linearizeChannel(g) +
    0.0722 * linearizeChannel(b)
  );
}

export function isDarkHex(hex: string): boolean {
  const lum = getRelativeLuminance(hex);
  if (lum === null) return false;
  return lum < 0.179;
}

export function getContrastRatio(hexA: string, hexB: string): number {
  const lumA = getRelativeLuminance(hexA);
  const lumB = getRelativeLuminance(hexB);
  if (lumA === null || lumB === null) return 1;
  const lighter = Math.max(lumA, lumB);
  const darker = Math.min(lumA, lumB);
  return (lighter + 0.05) / (darker + 0.05);
}

function pickBestContrastingColor(
  baseHex: string,
  candidates: readonly string[]
): string {
  let best = candidates[0];
  let bestRatio = getContrastRatio(baseHex, best);
  for (let i = 1; i < candidates.length; i++) {
    const ratio = getContrastRatio(baseHex, candidates[i]);
    if (ratio > bestRatio) {
      best = candidates[i];
      bestRatio = ratio;
    }
  }
  return best;
}

export interface ResolvedConsoleColors {
  fg: string | null;
  bg: string | null;
}

export function resolveConsoleColors(options: {
  fg: string | null | undefined;
  bg: string | null | undefined;
  theme?: ThemeMode;
}): ResolvedConsoleColors {
  const validFg = isValidHexColor(options.fg) ? options.fg.trim() : null;
  const validBg = isValidHexColor(options.bg) ? options.bg.trim() : null;
  const theme = options.theme ?? "light";

  if (!validFg && !validBg) {
    return { fg: null, bg: null };
  }

  if (!validFg && validBg) {
    const safeFg = pickBestContrastingColor(validBg, [
      SAFE_LIGHT_TEXT,
      SAFE_DARK_TEXT,
      "#ffffff",
      "#000000",
    ]);
    return { fg: safeFg, bg: validBg };
  }

  if (validFg && !validBg) {
    const defaultBg = DEFAULT_CONSOLE_BG[theme];
    if (getContrastRatio(validFg, defaultBg) >= 4.5) {
      return { fg: validFg, bg: null };
    }
    const safeBg = pickBestContrastingColor(validFg, [
      SAFE_DARK_BG,
      SAFE_LIGHT_BG,
      "#000000",
      "#ffffff",
    ]);
    return { fg: validFg, bg: safeBg };
  }

  // Both validFg and validBg are set: keep intentional contrasting pairs (>= 3.0, e.g. Amarelo on Azul = 4.38),
  // but prevent light-on-light / dark-on-dark unreadable combinations (< 3.0).
  if (validFg && validBg) {
    if (getContrastRatio(validFg, validBg) >= 3.0) {
      return { fg: validFg, bg: validBg };
    }
    const safeFg = pickBestContrastingColor(validBg, [
      SAFE_LIGHT_TEXT,
      SAFE_DARK_TEXT,
      "#ffffff",
      "#000000",
    ]);
    return { fg: safeFg, bg: validBg };
  }

  return { fg: null, bg: null };
}
