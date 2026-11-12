import { memo, useEffect, useRef } from "react";
import {
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
} from "@codemirror/autocomplete";
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
} from "@codemirror/commands";
import { bracketMatching, indentOnInput } from "@codemirror/language";
import {
  Compartment,
  EditorState,
  RangeSet,
  RangeSetBuilder,
  StateEffect,
  StateField,
  Text,
} from "@codemirror/state";
import {
  Decoration,
  DecorationSet,
  drawSelection,
  EditorView,
  gutter,
  GutterMarker,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  KeyBinding,
  lineNumbers,
} from "@codemirror/view";
import {
  algAutocompleteExtension,
  algAutoCloseEnterBinding,
  algLanguageExtension,
} from "../editor/algoritmosLanguage";
import type { ThemeMode } from "../types/ide";

class BreakpointMarker extends GutterMarker {
  toDOM(): Node {
    const el = document.createElement("span");
    el.className = "cm-bp-dot";
    el.setAttribute("aria-label", "Ponto de parada");
    el.textContent = "●";
    return el;
  }
}

const breakpointMarker = new BreakpointMarker();

const toggleBreakpointEffect = StateEffect.define<number>();
const setPausedLineEffect = StateEffect.define<number | null>();
const setErrorLineEffect = StateEffect.define<number | null>();

function normalizeMarkers(
  doc: Text,
  set: RangeSet<GutterMarker>
): RangeSet<GutterMarker> {
  const lineStarts = new Set<number>();
  const iter = set.iter();
  while (iter.value) {
    const line = doc.lineAt(Math.min(iter.from, doc.length));
    lineStarts.add(line.from);
    iter.next();
  }
  const builder = new RangeSetBuilder<GutterMarker>();
  const sorted = Array.from(lineStarts).sort((a, b) => a - b);
  for (const pos of sorted) {
    builder.add(pos, pos, breakpointMarker);
  }
  return builder.finish();
}

function extractBreakpointLines(
  doc: Text,
  set: RangeSet<GutterMarker>
): number[] {
  const lines: number[] = [];
  const iter = set.iter();
  while (iter.value) {
    lines.push(doc.lineAt(Math.min(iter.from, doc.length)).number);
    iter.next();
  }
  return lines;
}

const breakpointField = StateField.define<RangeSet<GutterMarker>>({
  create() {
    return RangeSet.empty;
  },
  update(markers, tr) {
    let next = markers;
    if (tr.docChanged) {
      next = normalizeMarkers(tr.newDoc, next.map(tr.changes));
    }
    for (const effect of tr.effects) {
      if (effect.is(toggleBreakpointEffect)) {
        const line = tr.newDoc.lineAt(Math.min(effect.value, tr.newDoc.length));
        let exists = false;
        const iter = next.iter(line.from);
        if (iter.value && iter.from === line.from) {
          exists = true;
        }
        next = next.update({
          filter: (from) => (exists ? from !== line.from : true),
          add: exists ? [] : [breakpointMarker.range(line.from)],
        });
      }
    }
    return next;
  },
});

const toggleBreakpointKeyBinding: KeyBinding = {
  key: "Mod-b",
  run(view) {
    const head = view.state.selection.main.head;
    const line = view.state.doc.lineAt(head);
    view.dispatch({
      effects: toggleBreakpointEffect.of(line.from),
    });
    return true;
  },
};

const activeLineDecoration = Decoration.line({ class: "cm-exec-paused-line" });
const errorLineDecoration = Decoration.line({ class: "cm-exec-error-line" });

const highlightLinesField = StateField.define<{
  pausedLine: number | null;
  errorLine: number | null;
  decos: DecorationSet;
}>({
  create() {
    return { pausedLine: null, errorLine: null, decos: Decoration.none };
  },
  update(state, tr) {
    let { pausedLine, errorLine } = state;
    let changed = tr.docChanged;

    for (const eff of tr.effects) {
      if (eff.is(setPausedLineEffect)) {
        pausedLine = eff.value;
        changed = true;
      } else if (eff.is(setErrorLineEffect)) {
        errorLine = eff.value;
        changed = true;
      }
    }

    if (!changed) return state;

    const ranges: { from: number; deco: Decoration }[] = [];
    if (
      pausedLine !== null &&
      pausedLine >= 1 &&
      pausedLine <= tr.newDoc.lines
    ) {
      const line = tr.newDoc.line(pausedLine);
      ranges.push({ from: line.from, deco: activeLineDecoration });
    }
    if (errorLine !== null && errorLine >= 1 && errorLine <= tr.newDoc.lines) {
      const line = tr.newDoc.line(errorLine);
      ranges.push({ from: line.from, deco: errorLineDecoration });
    }
    ranges.sort((a, b) => a.from - b.from);

    const builder = new RangeSetBuilder<Decoration>();
    for (const r of ranges) {
      builder.add(r.from, r.from, r.deco);
    }
    return { pausedLine, errorLine, decos: builder.finish() };
  },
  provide: (f) => EditorView.decorations.from(f, (v) => v.decos),
});

function buildAssistanceExtension(examMode: boolean) {
  if (examMode) {
    return [keymap.of([indentWithTab])];
  }
  return [
    algAutocompleteExtension,
    closeBrackets(),
    indentOnInput(),
    keymap.of([
      algAutoCloseEnterBinding,
      ...completionKeymap,
      ...closeBracketsKeymap,
      indentWithTab,
    ]),
  ];
}

function buildThemeExtension(theme: ThemeMode, fontSize: number) {
  return EditorView.theme(
    {
      "&": {
        height: "100%",
        fontSize: `${fontSize}px`,
        backgroundColor: "var(--editor-bg)",
        color: "var(--text-primary)",
      },
      ".cm-scroller": {
        fontFamily: "var(--font-mono)",
        lineHeight: "1.6",
        overflow: "auto",
      },
      ".cm-gutters": {
        backgroundColor: "var(--editor-gutter-bg)",
        color: "var(--text-muted)",
        borderRight: "1px solid var(--border-subtle)",
        userSelect: "none",
      },
      ".cm-activeLine": {
        backgroundColor: "var(--editor-active-line)",
      },
      ".cm-activeLineGutter": {
        backgroundColor: "var(--editor-active-line)",
        color: "var(--text-primary)",
      },
      ".cm-cursor": {
        borderLeftColor: "var(--accent)",
        borderLeftWidth: "2px",
      },
    },
    { dark: theme === "dark" }
  );
}

export interface JumpTarget {
  line: number;
  column?: number | null;
  seq: number;
}

interface CodeEditorProps {
  value: string;
  docResetKey: number;
  onChange: (nextValue: string) => void;
  onBreakpointsChange: (lines: number[]) => void;
  onCursorChange?: (line: number, column: number) => void;
  pausedLine: number | null;
  errorLine: number | null;
  jumpTarget: JumpTarget | null;
  examMode: boolean;
  theme: ThemeMode;
  fontSize: number;
  readOnly?: boolean;
}

export const CodeEditor = memo(function CodeEditor({
  value,
  docResetKey,
  onChange,
  onBreakpointsChange,
  onCursorChange,
  pausedLine,
  errorLine,
  jumpTarget,
  examMode,
  theme,
  fontSize,
  readOnly = false,
}: CodeEditorProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);

  const assistanceComp = useRef(new Compartment());
  const themeComp = useRef(new Compartment());
  const readOnlyComp = useRef(new Compartment());

  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const onBpRef = useRef(onBreakpointsChange);
  onBpRef.current = onBreakpointsChange;
  const onCursorRef = useRef(onCursorChange);
  onCursorRef.current = onCursorChange;

  const lastValueRef = useRef(value);
  const lastResetKeyRef = useRef(docResetKey);
  const lastBpSerializedRef = useRef("[]");

  const createEditorState = (
    docText: string,
    currentExam: boolean,
    currentTheme: ThemeMode,
    currentFontSize: number,
    currentReadOnly: boolean
  ) => {
    return EditorState.create({
      doc: docText,
      extensions: [
        breakpointField,
        gutter({
          class: "cm-bp-gutter",
          markers: (v) => v.state.field(breakpointField),
          initialSpacer: () => breakpointMarker,
          domEventHandlers: {
            mousedown(view, line, event) {
              if (!(event instanceof MouseEvent) || event.button !== 0) {
                return false;
              }
              view.dispatch({
                effects: toggleBreakpointEffect.of(line.from),
              });
              return true;
            },
          },
        }),
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightActiveLine(),
        drawSelection(),
        history(),
        bracketMatching(),
        algLanguageExtension,
        highlightLinesField,
        EditorView.contentAttributes.of({
          "aria-label": "Editor de código Portugol",
        }),
        readOnlyComp.current.of(EditorState.readOnly.of(currentReadOnly)),
        assistanceComp.current.of(buildAssistanceExtension(currentExam)),
        themeComp.current.of(buildThemeExtension(currentTheme, currentFontSize)),
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          toggleBreakpointKeyBinding,
        ]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            const text = update.state.doc.toString();
            lastValueRef.current = text;
            onChangeRef.current(text);
          }

          const bps = extractBreakpointLines(
            update.state.doc,
            update.state.field(breakpointField)
          );
          const serialized = JSON.stringify(bps);
          if (serialized !== lastBpSerializedRef.current) {
            lastBpSerializedRef.current = serialized;
            onBpRef.current(bps);
          }

          if (update.selectionSet || update.docChanged) {
            const head = update.state.selection.main.head;
            const line = update.state.doc.lineAt(head);
            onCursorRef.current?.(line.number, head - line.from + 1);
          }
        }),
      ],
    });
  };

  useEffect(() => {
    if (!hostRef.current) return;

    const view = new EditorView({
      state: createEditorState(value, examMode, theme, fontSize, readOnly),
      parent: hostRef.current,
    });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // Montagem única; atualizações subsequentes são feitas via efeitos/Compartments
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Reconfigura modo exame / tema / fonte / somente leitura dinamicamente
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: [
        assistanceComp.current.reconfigure(buildAssistanceExtension(examMode)),
        themeComp.current.reconfigure(buildThemeExtension(theme, fontSize)),
        readOnlyComp.current.reconfigure(EditorState.readOnly.of(readOnly)),
      ],
    });
  }, [examMode, theme, fontSize, readOnly]);

  // Troca completa de documento (Abrir / Novo / Exemplo) ou alteração externa
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;

    if (docResetKey !== lastResetKeyRef.current) {
      lastResetKeyRef.current = docResetKey;
      lastValueRef.current = value;
      lastBpSerializedRef.current = "[]";
      view.setState(
        createEditorState(value, examMode, theme, fontSize, readOnly)
      );
      onBpRef.current([]);
      onCursorRef.current?.(1, 1);
      return;
    }

    if (value !== lastValueRef.current) {
      lastValueRef.current = value;
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: value },
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value, docResetKey]);

  // Atualiza destaque da linha pausada e rola suavemente apenas se fora da área visível
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;

    const effects: StateEffect<unknown>[] = [setPausedLineEffect.of(pausedLine)];
    if (
      pausedLine !== null &&
      pausedLine >= 1 &&
      pausedLine <= view.state.doc.lines
    ) {
      const line = view.state.doc.line(pausedLine);
      const scrollRect = view.scrollDOM.getBoundingClientRect();
      const coords = view.coordsAtPos(line.from);
      const isOutsideViewport =
        scrollRect.height === 0 ||
        !coords ||
        coords.top < scrollRect.top + 8 ||
        coords.bottom > scrollRect.bottom - 8;
      if (isOutsideViewport) {
        effects.push(EditorView.scrollIntoView(line.from, { y: "nearest" }));
      }
    }
    view.dispatch({ effects });
  }, [pausedLine]);

  // Atualiza destaque da linha de erro
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: setErrorLineEffect.of(errorLine),
    });
  }, [errorLine]);

  // Saltar para linha:coluna (clique em diagnóstico no console)
  useEffect(() => {
    const view = viewRef.current;
    if (!view || !jumpTarget) return;
    const clampedLine = Math.max(
      1,
      Math.min(jumpTarget.line, view.state.doc.lines)
    );
    const line = view.state.doc.line(clampedLine);
    const colOffset = jumpTarget.column
      ? Math.max(0, jumpTarget.column - 1)
      : 0;
    const pos = Math.min(line.from + colOffset, line.to);
    view.dispatch({
      selection: { anchor: pos },
      effects: EditorView.scrollIntoView(pos, { y: "center" }),
    });
    view.focus();
  }, [jumpTarget]);

  return (
    <div className="code-editor-shell">
      {readOnly && (
        <div className="editor-readonly-notice" role="status">
          Editor bloqueado durante a execução — pare ou conclua para editar
          (Ctrl+B alterna breakpoints)
        </div>
      )}
      <div ref={hostRef} className="code-editor-host" />
    </div>
  );
});
