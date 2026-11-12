const ROOT_PATH_RE =
  /^(?:[/\\]+|[A-Za-z]:[/\\]?|\\\\\?\\[A-Za-z]:[/\\]?|\\\\(?:\?\\(?:UNC\\)?)?[^/\\]*[/\\]?|\\\\(?:\?\\UNC\\)?[^/\\?][^/\\]*[/\\][^/\\]+[/\\]?)$/i;

export function getParentDir(filePath: string): string | null {
  if (!filePath || ROOT_PATH_RE.test(filePath)) return null;
  const match = filePath.match(/^(.*)[/\\][^/\\]+$/);
  if (!match) return null;
  if (match[1] === "") return filePath.startsWith("/") ? "/" : null;
  if (/^(?:\\\\\?\\)?[A-Za-z]:$/.test(match[1])) return `${match[1]}\\`;
  return match[1];
}

export function getFileName(path: string, fallback = ""): string {
  if (!path) return fallback;
  const trimmed = path.replace(/[/\\]+$/, "");
  if (!trimmed) {
    return path.startsWith("/") ? "/" : fallback || path;
  }
  if (/^(?:\\\\\?\\)?[A-Za-z]:$/.test(trimmed)) {
    return `${trimmed}\\`;
  }
  const parts = trimmed.split(/[/\\]/);
  return parts[parts.length - 1] || fallback || trimmed;
}
