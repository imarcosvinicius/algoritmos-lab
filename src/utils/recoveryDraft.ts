export const RECOVERY_STORAGE_KEY = "algoritmoslab.recovery.v1";

let latestInMemorySource: string | null = null;

export function setRecoverySource(source: string | null): void {
  latestInMemorySource = source;
  try {
    if (typeof window === "undefined") return;
    if (source === null) {
      window.localStorage.removeItem(RECOVERY_STORAGE_KEY);
    } else {
      window.localStorage.setItem(RECOVERY_STORAGE_KEY, source);
    }
  } catch {
    // Ignora erros de armazenamento
  }
}

export function getRecoverySource(): string | null {
  if (latestInMemorySource !== null) return latestInMemorySource;
  try {
    if (typeof window === "undefined") return null;
    return window.localStorage.getItem(RECOVERY_STORAGE_KEY);
  } catch {
    return null;
  }
}
