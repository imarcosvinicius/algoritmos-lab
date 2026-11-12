import { BookOpen } from "lucide-react";
import { DIDACTIC_EXAMPLES } from "../data/examples";
import type { AlgorithmExample } from "../types/ide";
import { ModalDialog } from "./ModalDialog";

interface ExamplesModalProps {
  open: boolean;
  onClose: () => void;
  onSelectExample: (example: AlgorithmExample) => void;
}

export function ExamplesModal({
  open,
  onClose,
  onSelectExample,
}: ExamplesModalProps) {
  return (
    <ModalDialog
      open={open}
      title="Exemplos Didáticos (.alg)"
      titleId="examples-modal-title"
      icon={<BookOpen size={18} aria-hidden="true" />}
      closeLabel="Fechar exemplos"
      size="lg"
      onClose={onClose}
    >
      <div className="modal-body">
        <p className="modal-subtitle">
          Escolha um algoritmo pronto para estudar, executar (<kbd>F9</kbd>) ou
          depurar passo a passo (<kbd>F8</kbd>).
        </p>

        <div className="examples-grid">
          {DIDACTIC_EXAMPLES.map((ex) => (
            <article key={ex.id} className="example-card">
              <div className="example-card-top">
                <h3>{ex.title}</h3>
                <span className="example-level">{ex.level}</span>
              </div>
              <p className="example-desc">{ex.description}</p>
              <div className="example-tags">
                {ex.tags.map((tag) => (
                  <span key={tag} className="example-tag">
                    {tag}
                  </span>
                ))}
              </div>
              <button
                type="button"
                className="btn-primary-sm example-load-btn"
                onClick={() => onSelectExample(ex)}
              >
                Carregar no Editor
              </button>
            </article>
          ))}
        </div>
      </div>
    </ModalDialog>
  );
}
