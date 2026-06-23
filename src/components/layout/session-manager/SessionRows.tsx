import type { MouseEventHandler, ReactNode } from "react";
import { ChevronRight } from "lucide-react";

import { Tooltip } from "../../../components/ui";

export function GroupRow({
  actions,
  children,
  expanded,
  icon,
  label,
  onToggle,
  tooltip,
}: {
  actions?: ReactNode;
  badge?: ReactNode;
  children: ReactNode;
  expanded: boolean;
  icon?: ReactNode;
  label: ReactNode;
  onToggle: () => void;
  tooltip?: string;
}) {
  const labelNode = (
    <span className="block min-w-0 truncate text-[13px] uppercase leading-none text-foreground-secondary">
      {label}
    </span>
  );

  return (
    <div>
      <div
        role="button"
        tabIndex={0}
        onClick={onToggle}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onToggle();
          }
        }}
        className="group relative flex h-[35px] w-full rounded-sm cursor-pointer items-center gap-1.5 px-2.5 py-2.5 text-left text-foreground-secondary transition hover:bg-card"
        style={{ paddingLeft: 10, paddingRight: actions ? 54 : 10 }}
      >
        {icon && (
          <span className="grid h-[15px] w-[15px] shrink-0 place-items-center text-foreground-muted">
            {icon}
          </span>
        )}
        {tooltip ? <Tooltip content={tooltip}>{labelNode}</Tooltip> : labelNode}
        <ChevronRight
          className={`h-3.5 w-3.5 shrink-0 text-foreground-muted opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100 ${expanded ? "rotate-90" : ""}`}
          aria-hidden="true"
        />
        <span className="min-w-0 flex-1" />
        {actions && (
          <span className="absolute right-2.5 top-1/2 -translate-y-1/2">
            {actions}
          </span>
        )}
      </div>
      {expanded && children}
    </div>
  );
}

export function ItemRow({
  actions,
  active,
  label,
  onClick,
  onContextMenu,
  tooltip,
  trailing,
}: {
  actions?: ReactNode;
  active?: boolean;
  label: ReactNode;
  onClick: () => void;
  onContextMenu?: MouseEventHandler<HTMLDivElement>;
  tooltip?: string;
  trailing?: ReactNode;
}) {
  const rowTone = active
    ? "bg-primary font-medium text-[#ffffff]"
    : "text-foreground hover:bg-accent hover:text-accent-foreground";
  const labelNode = (
    <span className="block min-w-0 flex-1 truncate text-[13px] leading-none">
      {label}
    </span>
  );

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onClick();
        }
      }}
      onContextMenu={onContextMenu}
      className={`group relative text-[13px] flex h-[34px] w-full cursor-pointer items-center gap-1.5 rounded-sm py-2.5 text-left transition ${actions ? "pr-7" : "pr-1"} ${rowTone}`}
      style={{ paddingLeft: 25 }}
    >
      {tooltip ? <Tooltip content={tooltip}>{labelNode}</Tooltip> : labelNode}
      {trailing}
      {actions && (
        <span className="absolute right-1 top-1/2 -translate-y-1/2">
          {actions}
        </span>
      )}
    </div>
  );
}
