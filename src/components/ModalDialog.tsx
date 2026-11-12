import { useEffect, useRef, type ReactNode } from "react";
import { X } from "lucide-react";

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex="-1"])',
].join(", ");

function getFocusableElements(container: HTMLElement): HTMLElement[] {
  const candidates = Array.from(
    container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)
  );
  return candidates.filter((el) => {
    if (el.hasAttribute("disabled")) return false;
    if (el.getAttribute("aria-hidden") === "true") return false;
    if (el.closest("[hidden]")) return false;
    return true;
  });
}

interface ModalDialogProps {
  open: boolean;
  title: string;
  titleId: string;
  icon?: ReactNode;
  closeLabel: string;
  size?: "md" | "lg";
  onClose: () => void;
  children: ReactNode;
}

export function ModalDialog({
  open,
  title,
  titleId,
  icon,
  closeLabel,
  size = "md",
  onClose,
  children,
}: ModalDialogProps) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const previousActiveElementRef = useRef<HTMLElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    if (!open) return;

    previousActiveElementRef.current =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;

    const dialogEl = dialogRef.current;
    if (dialogEl) {
      const focusables = getFocusableElements(dialogEl);
      if (focusables.length > 0) {
        focusables[0].focus();
      } else {
        dialogEl.focus();
      }
    }

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onCloseRef.current();
        return;
      }

      if (e.key === "Tab" && dialogRef.current) {
        const focusables = getFocusableElements(dialogRef.current);
        if (focusables.length === 0) {
          e.preventDefault();
          dialogRef.current.focus();
          return;
        }
        const first = focusables[0];
        const last = focusables[focusables.length - 1];
        const active = document.activeElement;

        if (e.shiftKey) {
          if (active === first || !dialogRef.current.contains(active)) {
            e.preventDefault();
            last.focus();
          }
        } else if (active === last || !dialogRef.current.contains(active)) {
          e.preventDefault();
          first.focus();
        }
      }
    };

    document.addEventListener("keydown", onKeyDown, true);

    return () => {
      document.removeEventListener("keydown", onKeyDown, true);
      const prev = previousActiveElementRef.current;
      previousActiveElementRef.current = null;
      if (prev && prev.isConnected) {
        prev.focus();
      }
    };
  }, [open]);

  if (!open) return null;

  return (
    <div
      className="modal-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) {
          onClose();
        }
      }}
    >
      <div
        ref={dialogRef}
        className={`modal-card ${size === "lg" ? "modal-card-lg" : ""}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
      >
        <header className="modal-header">
          <div className="modal-title-wrap">
            {icon}
            <h2 id={titleId}>{title}</h2>
          </div>
          <button
            type="button"
            className="tb-icon-btn"
            onClick={onClose}
            aria-label={closeLabel}
          >
            <X size={18} aria-hidden="true" />
          </button>
        </header>

        {children}
      </div>
    </div>
  );
}
