import type { MouseEventHandler, ReactNode } from "react";
import { ChevronDown, type LucideIcon } from "lucide-react";
import { Tooltip } from "./Tooltip";
import { joinClasses } from "./utils";

export type SplitButtonTon = "card" | "surface";

export function SplitButton({
  className,
  disabled = false,
  icon: Icon,
  label,
  menuAriaLabel,
  menuDisabled,
  menuTooltip,
  onClick,
  onMenuClick,
  ton = "surface",
  tooltip,
}: {
  className?: string;
  disabled?: boolean;
  icon: LucideIcon;
  label: ReactNode;
  menuAriaLabel: string;
  menuDisabled?: boolean;
  menuTooltip: ReactNode;
  onClick?: MouseEventHandler<HTMLButtonElement>;
  onMenuClick?: MouseEventHandler<HTMLButtonElement>;
  ton?: SplitButtonTon;
  tooltip: ReactNode;
}) {
  const containerClass =
    ton === "card" ? "bg-card" : "bg-surface";
  const resolvedMenuDisabled = menuDisabled ?? disabled;

  return (
    <div
      className={joinClasses(
        "flex h-8 shrink-0 overflow-hidden rounded-lg border border-border text-sm shadow-sm",
        containerClass,
        className
      )}
    >
      <Tooltip content={tooltip}>
        <button
          type="button"
          disabled={disabled}
          className="flex items-center gap-1.5 px-3 font-medium text-foreground transition hover:bg-accent disabled:cursor-not-allowed disabled:opacity-45"
          onClick={onClick}
        >
          <Icon className="h-3.5 w-3.5" aria-hidden="true" />
          {label}
        </button>
      </Tooltip>
      <Tooltip content={menuTooltip}>
        <button
          type="button"
          aria-label={menuAriaLabel}
          disabled={resolvedMenuDisabled}
          className="grid w-8 place-items-center border-l border-border text-foreground-muted transition hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:opacity-45"
          onClick={onMenuClick}
        >
          <ChevronDown className="h-4 w-4" aria-hidden="true" />
        </button>
      </Tooltip>
    </div>
  );
}
