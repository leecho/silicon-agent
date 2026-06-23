import { X } from "lucide-react";
import { useEffect, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Tooltip } from "./Tooltip";
import { joinClasses } from "./utils";

export function Drawer({
  children,
  className,
  onClose,
  open,
  title,
  widthClassName = "w-[min(980px,92vw)]",
  width,
}: {
  children: ReactNode;
  className?: string;
  onClose: () => void;
  open: boolean;
  title?: string;
  width?: string;
  widthClassName?: string;
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

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex justify-end bg-black/40"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <aside
        aria-label={title}
        className={joinClasses(
          "grid h-full grid-rows-[auto_minmax(0,1fr)] border-l border-border bg-background text-popover-foreground shadow-2xl",
          width ? null : widthClassName,
          className,
        )}
        role="dialog"
        style={width ? { width } : undefined}
      >
        {children}
      </aside>
    </div>,
    document.body,
  );
}

export function DrawerHeader({ children, onClose }: { children: ReactNode; onClose: () => void }) {
  return (
    <header className="flex items-center justify-between bg-surface gap-4 border-b border-border px-5 py-2">
      <div className="min-w-0 flex-1">{children}</div>
      <Tooltip content="关闭">
        <button
          type="button"
          aria-label="关闭"
          className="grid h-8 w-8 shrink-0 place-items-center rounded-lg text-foreground-muted transition hover:bg-accent hover:text-accent-foreground"
          onClick={onClose}
        >
          <X className="h-4 w-4 stroke-current" aria-hidden="true" />
        </button>
      </Tooltip>
    </header>
  );
}
