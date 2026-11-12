import { memo, useEffect, useState } from "react";
import { ChevronDown, ChevronRight, Layers, Variable } from "lucide-react";
import type { VarSnapshot } from "../types/ide";

interface VariableWatchPanelProps {
  variables: VarSnapshot[];
  callStack: string[];
  isPaused: boolean;
  resetKey?: string | number;
}

export const VariableWatchPanel = memo(function VariableWatchPanel({
  variables,
  callStack,
  isPaused,
  resetKey,
}: VariableWatchPanelProps) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});

  useEffect(() => {
    setExpanded({});
  }, [resetKey]);

  const toggleExpand = (key: string) => {
    setExpanded((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  return (
    <aside className="watch-panel" aria-label="Variáveis e pilha de chamadas">
      {callStack.length > 0 && (
        <section className="watch-section">
          <header className="panel-header">
            <span className="panel-title">
              <Layers size={14} aria-hidden="true" />
              Pilha de Chamadas
            </span>
            {isPaused && <span className="badge badge-paused">Pausado</span>}
          </header>
          <ol className="callstack-list">
            {[...callStack].reverse().map((frame, idx) => {
              const depthIdx = callStack.length - 1 - idx;
              return (
                <li
                  key={`${frame}:${depthIdx}`}
                  className={`callstack-item ${idx === 0 ? "active" : ""}`}
                >
                  <span className="callstack-depth">#{depthIdx}</span>
                  <span className="callstack-name">{frame}</span>
                </li>
              );
            })}
          </ol>
        </section>
      )}

      <section className="watch-section watch-section-vars">
        <header className="panel-header">
          <span className="panel-title">
            <Variable size={14} aria-hidden="true" />
            Variáveis ({variables.length})
          </span>
        </header>

        {variables.length === 0 ? (
          <div className="empty-state">
            Execute ou depure o algoritmo para acompanhar os valores das
            variáveis em tempo real.
          </div>
        ) : (
          <div className="vars-table-wrap">
            <table className="vars-table">
              <thead>
                <tr>
                  <th>Nome</th>
                  <th>Tipo</th>
                  <th>Valor</th>
                </tr>
              </thead>
              <tbody>
                {variables.map((v, idx) => {
                  const rowKey = `${v.scope}:${v.name}:${idx}`;
                  const hasItems = Boolean(v.items && v.items.length > 0);
                  const isOpen = Boolean(expanded[rowKey]);
                  const showTruncationNote =
                    hasItems &&
                    v.total_items !== undefined &&
                    v.items !== undefined &&
                    v.total_items > v.items.length;

                  return (
                    <VariableRowGroup
                      key={rowKey}
                      variable={v}
                      hasItems={hasItems}
                      isOpen={isOpen}
                      showTruncationNote={showTruncationNote}
                      onToggle={() => toggleExpand(rowKey)}
                    />
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </aside>
  );
});

interface VariableRowGroupProps {
  variable: VarSnapshot;
  hasItems: boolean;
  isOpen: boolean;
  showTruncationNote: boolean;
  onToggle: () => void;
}

function VariableRowGroup({
  variable,
  hasItems,
  isOpen,
  showTruncationNote,
  onToggle,
}: VariableRowGroupProps) {
  return (
    <>
      <tr className={hasItems ? "var-row var-row-expandable" : "var-row"}>
        <td className="var-name-cell">
          {hasItems ? (
            <button
              type="button"
              className="var-expand-btn"
              onClick={onToggle}
              aria-expanded={isOpen}
              title={isOpen ? "Recolher itens" : "Expandir itens/campos"}
            >
              {isOpen ? (
                <ChevronDown size={13} aria-hidden="true" />
              ) : (
                <ChevronRight size={13} aria-hidden="true" />
              )}
              <span>{variable.name}</span>
            </button>
          ) : (
            <span>{variable.name}</span>
          )}
          {variable.scope !== "global" && (
            <span
              className="var-scope-tag"
              title={`Escopo: ${variable.scope}`}
            >
              {variable.scope}
            </span>
          )}
        </td>
        <td className="var-type-cell">{variable.type_label}</td>
        <td className="var-value-cell" title={variable.value_text}>
          {variable.value_text === "" ? '""' : variable.value_text}
        </td>
      </tr>

      {hasItems && isOpen && variable.items && (
        <tr className="var-items-row">
          <td colSpan={3}>
            <div className="var-items-grid">
              {variable.items.map((item, itemIdx) => (
                <div
                  key={`${item.index_label}:${itemIdx}`}
                  className="var-item-chip"
                >
                  <span className="var-item-idx">{item.index_label}</span>
                  <span className="var-item-val">
                    {item.value_text === "" ? '""' : item.value_text}
                  </span>
                </div>
              ))}
            </div>
            {showTruncationNote && (
              <div className="var-items-truncated">
                Mostrando os primeiros {variable.items.length} de{" "}
                {variable.total_items} elementos.
              </div>
            )}
          </td>
        </tr>
      )}
    </>
  );
}
