import { X } from "lucide-react";
import { useEffect, type ReactNode } from "react";
import { joinClasses } from "./utils";

export function Modal({
  children,
  className,
  onClose,
  open,
  padding = "default",
  title
}: {
  children: ReactNode;
  className?: string;
  onClose: () => void;
  open: boolean;
  padding?: "default" | "none";
  title?: string;
}) {
  useEffect(() => {
    if (!open) return;

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose, open]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-[60] grid place-items-center bg-black/40 px-4"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section
        aria-label={title}
        aria-modal="true"
        className={joinClasses(
          "w-full max-w-xl rounded-lg border border-border bg-popover text-popover-foreground shadow-2xl",
          padding === "default" ? "p-5" : false,
          className,
        )}
        role="dialog"
      >
        {children}
      </section>
    </div>
  );
}

export function ModalHeader({ children, onClose }: { children: ReactNode; onClose: () => void }) {
  return (
    <div className="flex items-start justify-between gap-4">
      <div className="min-w-0">{children}</div>
      <button
        aria-label="关闭"
        className="grid h-8 w-8 shrink-0 place-items-center rounded-lg text-popover-foreground transition hover:bg-accent hover:text-accent-foreground"
        type="button"
        onClick={onClose}
      >
        <X className="h-4 w-4" aria-hidden="true" />
      </button>
    </div>
  );
}
