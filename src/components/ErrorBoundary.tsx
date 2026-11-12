import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle, Check, Copy, RefreshCw } from "lucide-react";
import { getRecoverySource } from "../utils/recoveryDraft";

interface ErrorBoundaryProps {
  children: ReactNode;
  onReload?: () => void;
}

interface ErrorBoundaryState {
  hasError: boolean;
  errorMessage: string;
  preservedSource: string | null;
  copied: boolean;
  copyFailed: boolean;
}

export class ErrorBoundary extends Component<
  ErrorBoundaryProps,
  ErrorBoundaryState
> {
  state: ErrorBoundaryState = {
    hasError: false,
    errorMessage: "",
    preservedSource: null,
    copied: false,
    copyFailed: false,
  };

  static getDerivedStateFromError(error: unknown): Partial<ErrorBoundaryState> {
    const message =
      error instanceof Error
        ? error.message
        : typeof error === "string"
        ? error
        : "Erro desconhecido na interface.";
    return {
      hasError: true,
      errorMessage: message,
      preservedSource: getRecoverySource(),
      copied: false,
      copyFailed: false,
    };
  }

  componentDidCatch(error: unknown, info: ErrorInfo): void {
    console.error("Erro inesperado capturado pelo ErrorBoundary:", error, info);
  }

  private handleCopy = async (): Promise<void> => {
    const source = this.state.preservedSource;
    if (!source) return;
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(source);
        this.setState({ copied: true, copyFailed: false });
        return;
      }
      this.setState({ copied: false, copyFailed: true });
    } catch {
      this.setState({ copied: false, copyFailed: true });
    }
  };

  private handleReload = (): void => {
    if (this.props.onReload) {
      this.props.onReload();
      return;
    }
    window.location.reload();
  };

  private handleRecover = (): void => {
    this.setState({
      hasError: false,
      errorMessage: "",
      copied: false,
      copyFailed: false,
    });
  };

  render(): ReactNode {
    if (!this.state.hasError) {
      return this.props.children;
    }

    const { errorMessage, preservedSource, copied, copyFailed } = this.state;

    return (
      <main className="error-boundary-screen" role="alert">
        <section className="error-boundary-card">
          <header className="error-boundary-header">
            <AlertTriangle size={22} aria-hidden="true" />
            <div>
              <h1>Algo inesperado aconteceu na interface</h1>
              <p>
                A execução visual foi interrompida para evitar perda de dados.
              </p>
            </div>
          </header>

          <p className="error-boundary-message">
            <strong>Detalhes:</strong> {errorMessage}
          </p>

          {preservedSource !== null && preservedSource.length > 0 && (
            <div className="error-boundary-recovery">
              <label htmlFor="recovery-source-preview">
                Código não salvo preservado:
              </label>
              <textarea
                id="recovery-source-preview"
                className="error-boundary-textarea"
                readOnly
                value={preservedSource}
                rows={10}
              />
            </div>
          )}

          <div className="error-boundary-actions">
            {preservedSource !== null && preservedSource.length > 0 && (
              <button
                type="button"
                className="tb-btn"
                onClick={() => void this.handleCopy()}
              >
                {copied ? (
                  <Check size={15} aria-hidden="true" />
                ) : (
                  <Copy size={15} aria-hidden="true" />
                )}
                <span>{copied ? "Código copiado!" : "Copiar código"}</span>
              </button>
            )}
            <button
              type="button"
              className="tb-btn tb-btn-run"
              onClick={this.handleReload}
            >
              <RefreshCw size={15} aria-hidden="true" />
              <span>Recarregar IDE</span>
            </button>
            <button
              type="button"
              className="tb-btn"
              onClick={this.handleRecover}
            >
              <span>Tentar restaurar</span>
            </button>
          </div>

          {copyFailed && (
            <p className="error-boundary-hint">
              Não foi possível acessar a área de transferência automaticamente.
              Selecione e copie o código diretamente da caixa acima.
            </p>
          )}
        </section>
      </main>
    );
  }
}
