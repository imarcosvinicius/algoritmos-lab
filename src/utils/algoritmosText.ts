export function normalizeAlgWord(word: string): string {
  return word
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase();
}

export function normalizeLineEndings(text: string): string {
  return text.replace(/\r\n?/g, "\n");
}

export function stripCommentsAndMaskStrings(source: string): string {
  return source.replace(/\/\/[^\n]*|"(?:[^"\n]*)"/g, (match) =>
    match.startsWith('"') ? '""' : ""
  );
}

export function suggestFileName(
  source: string,
  fallback = "algoritmo.alg"
): string {
  const match = source.match(/algoritmo\s+"([^"]+)"/i);
  if (match && match[1].trim()) {
    const clean = match[1]
      .trim()
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .toLowerCase()
      .replace(/[^a-z0-9_-]+/g, "_")
      .replace(/^_+|_+$/g, "");
    if (clean) return `${clean}.alg`;
  }
  return fallback;
}

export function usesArquivoCommand(source: string): boolean {
  const masked = stripCommentsAndMaskStrings(source);
  const norm = normalizeAlgWord(masked);
  const beforeEnd = norm.replace(/\bfimalgoritmo\b[\s\S]*$/i, "");
  return /\barquivo\s*(?:\(\s*)?""/.test(beforeEnd);
}
