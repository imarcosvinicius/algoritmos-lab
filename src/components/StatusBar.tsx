import { memo, useEffect, useState } from "react";
import type { SourceEncoding } from "../types/ide";

export interface CursorPosition {
  line: number;
  column: number;
}

export type CursorSubscribeFn = (
  listener: (pos: CursorPosition) => void
) => () => void;

interface StatusBarProps {
  coreVersion: string;
  subscribeCursor: CursorSubscribeFn;
  breakpointCount: number;
  examMode: boolean;
  encoding: SourceEncoding;
}

export const StatusBar = memo(function StatusBar({
  coreVersion,
  subscribeCursor,
  breakpointCount,
  examMode,
  encoding,
}: StatusBarProps) {
  const [cursorPos, setCursorPos] = useState<CursorPosition>({
    line: 1,
    column: 1,
  });

  useEffect(() => {
    return subscribeCursor((pos) => {
      setCursorPos((prev) =>
        prev.line === pos.line && prev.column === pos.column ? prev : pos
      );
    });
  }, [subscribeCursor]);

  return (
    <footer className="ide-statusbar">
      <div className="statusbar-left">
        <span>algoritmoslab-core v{coreVersion}</span>
        <span aria-hidden="true">•</span>
        <span>
          Ln {cursorPos.line}, Col {cursorPos.column}
        </span>
        {breakpointCount > 0 && (
          <>
            <span aria-hidden="true">•</span>
            <span>
              {breakpointCount} breakpoint{breakpointCount > 1 ? "s" : ""}
            </span>
          </>
        )}
      </div>
      <div className="statusbar-right">
        <span>
          {examMode ? "Modo Prova (Editor Neutro)" : "Assistência ativa"}
        </span>
        <span aria-hidden="true">•</span>
        <span>{encoding === "utf8" ? "UTF-8" : "Windows-1252"}</span>
      </div>
    </footer>
  );
});
