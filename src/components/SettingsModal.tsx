import { Settings } from "lucide-react";
import type { IdeSettings } from "../types/ide";
import { ModalDialog } from "./ModalDialog";

interface SettingsModalProps {
  open: boolean;
  settings: IdeSettings;
  onClose: () => void;
  onUpdate: (patch: Partial<IdeSettings>) => void;
}

export function SettingsModal({
  open,
  settings,
  onClose,
  onUpdate,
}: SettingsModalProps) {
  return (
    <ModalDialog
      open={open}
      title="Configurações"
      titleId="settings-modal-title"
      icon={<Settings size={18} aria-hidden="true" />}
      closeLabel="Fechar configurações"
      onClose={onClose}
    >
      <div className="modal-body settings-list">
        <div className="setting-row">
          <div>
            <label htmlFor="cfg-theme" className="setting-label">
              Aparência da IDE
            </label>
            <p id="cfg-theme-help" className="setting-help">
              Alterne entre o tema claro e o tema escuro.
            </p>
          </div>
          <select
            id="cfg-theme"
            aria-describedby="cfg-theme-help"
            value={settings.theme}
            onChange={(e) =>
              onUpdate({ theme: e.target.value === "dark" ? "dark" : "light" })
            }
          >
            <option value="light">Claro</option>
            <option value="dark">Escuro</option>
          </select>
        </div>

        <div className="setting-row">
          <div>
            <label htmlFor="cfg-fontsize" className="setting-label">
              Tamanho da fonte do editor
            </label>
            <p id="cfg-fontsize-help" className="setting-help">
              Atual: {settings.fontSize}px
            </p>
          </div>
          <input
            id="cfg-fontsize"
            aria-describedby="cfg-fontsize-help"
            type="range"
            min={12}
            max={22}
            value={settings.fontSize}
            onChange={(e) => onUpdate({ fontSize: Number(e.target.value) })}
          />
        </div>

        <div className="setting-check-row">
          <div>
            <label htmlFor="cfg-exam-mode" className="setting-label">
              Modo Prova / Editor Neutro
            </label>
            <p id="cfg-exam-mode-help" className="setting-help">
              Desativa sugestões de autocomplete e fechamento automático de
              blocos (se/fimse, para/fimpara).
            </p>
          </div>
          <input
            id="cfg-exam-mode"
            type="checkbox"
            aria-describedby="cfg-exam-mode-help"
            checked={settings.examMode}
            onChange={(e) => onUpdate({ examMode: e.target.checked })}
          />
        </div>

        <div className="setting-check-row">
          <div>
            <label htmlFor="cfg-strict-read" className="setting-label">
              Leitura estrita no leia()
            </label>
            <p id="cfg-strict-read-help" className="setting-help">
              Se ativado, interrompe com erro ao digitar texto inválido para
              inteiro/real em vez de solicitar redigitação.
            </p>
          </div>
          <input
            id="cfg-strict-read"
            type="checkbox"
            aria-describedby="cfg-strict-read-help"
            checked={settings.strictRead}
            onChange={(e) => onUpdate({ strictRead: e.target.checked })}
          />
        </div>

        <div className="setting-row">
          <div>
            <label htmlFor="cfg-encoding" className="setting-label">
              Encoding padrão ao salvar
            </label>
            <p id="cfg-encoding-help" className="setting-help">
              UTF-8 é o padrão moderno; Windows-1252 garante compatibilidade
              direta com ambientes legados em Windows.
            </p>
          </div>
          <select
            id="cfg-encoding"
            aria-describedby="cfg-encoding-help"
            value={settings.defaultSaveEncoding}
            onChange={(e) =>
              onUpdate({
                defaultSaveEncoding:
                  e.target.value === "windows1252" ? "windows1252" : "utf8",
              })
            }
          >
            <option value="utf8">UTF-8 (Padrão)</option>
            <option value="windows1252">Windows-1252 (Clássico)</option>
          </select>
        </div>
      </div>
    </ModalDialog>
  );
}
