import type { ReactNode, MouseEvent } from "react";
import { ChevronRight } from "lucide-react";
import { Tooltip } from "../ui";

export interface SessionTreeNode {
  id: string;
  label: ReactNode;
  tooltip?: string;
  icon?: ReactNode;
  badge?: ReactNode;
  trailing?: ReactNode;
  actions?: ReactNode;
  active?: boolean;
  disabled?: boolean;
  loading?: boolean;
  expanded?: boolean;
  children?: SessionTreeNode[];
  onClick?: () => void;
  onContextMenu?: (event: MouseEvent<HTMLDivElement>) => void;
}

export interface SessionTreeProps {
  title: ReactNode;
  titleBadge?: ReactNode;
  expanded: boolean;
  nodes: SessionTreeNode[];
  className?: string;
  emptyLabel?: ReactNode;
  onToggle: () => void;
}

export interface SessionTreeContentProps {
  nodes: SessionTreeNode[];
  emptyLabel?: ReactNode;
}

function nodePadding(depth: number): number {
  return 10 + (depth - 1) * 15;
}

function groupHeaderPadding(depth: number): number {
  return 10 + (depth - 1) * 15;
}

function SessionTreeRow({
  node,
  depth,
}: {
  node: SessionTreeNode;
  depth: number;
}) {
  const isGroup = node.children !== undefined;
  const expanded = node.expanded ?? true;
  const interactive = !!node.onClick || !!node.onContextMenu || isGroup;
  const rowTone = isGroup
    ? "text-foreground-muted hover:text-foreground"
    : node.active
    ? "bg-primary font-medium text-[#ffffff]"
    : "text-foreground-secondary hover:bg-accent hover:text-accent-foreground";
  const labelClassName = isGroup
    ? "block min-w-0 truncate text-[13px] font-semibold uppercase leading-none text-foreground-muted"
    : "block min-w-0 flex-1 truncate text-[13px] leading-none";
  const label = (
    <span className={labelClassName}>
      {node.label}
    </span>
  );

  return (
    <div>
      <div
        role={interactive ? "button" : undefined}
        tabIndex={interactive && !node.disabled ? 0 : undefined}
        aria-disabled={node.disabled || undefined}
        onClick={node.onClick}
        onKeyDown={(event) => {
          if (!interactive || node.disabled) return;
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            node.onClick?.();
          }
        }}
        onContextMenu={node.onContextMenu}
        className={`group flex w-full items-center gap-1.5 text-left transition ${
          isGroup ? "pr-2.5 pb-1 pt-2" : "h-[34px] rounded-sm py-1 pr-1"
        } ${
          interactive ? "cursor-pointer" : "cursor-default"
        } ${node.disabled ? "opacity-60" : ""} ${rowTone}`}
        style={{ paddingLeft: isGroup ? groupHeaderPadding(depth) : nodePadding(depth) }}
      >
        {node.icon && (
          <span className="grid h-[15px] w-[15px] shrink-0 place-items-center text-foreground-muted">
            {node.icon}
          </span>
        )}
        {node.tooltip ? <Tooltip content={node.tooltip}>{label}</Tooltip> : label}
        {node.badge !== undefined && isGroup && (
          <>
            <span className="shrink-0 text-xs font-normal text-foreground-muted">·</span>
            <span className="shrink-0 text-xs font-normal text-foreground-muted">
              {node.badge}
            </span>
          </>
        )}
        {node.badge !== undefined && !isGroup && (
          <span className="shrink-0 rounded-full bg-muted px-1.5 py-0.5 text-[11px] font-normal text-foreground-muted">
            {node.badge}
          </span>
        )}
        {node.trailing}
        {isGroup && (
          <ChevronRight
            className={`h-3.5 w-3.5 shrink-0 text-foreground-muted transition ${
              expanded ? "rotate-90" : ""
            }`}
            aria-hidden="true"
          />
        )}
        {isGroup && <span className="min-w-0 flex-1" />}
        {node.actions}
      </div>
      {isGroup && expanded && (
        <div className="flex flex-col gap-0.5">
          {node.children?.map((child) => (
            <SessionTreeRow key={child.id} node={child} depth={depth + 1} />
          ))}
        </div>
      )}
    </div>
  );
}

export function SessionTree({
  title,
  titleBadge,
  expanded,
  nodes,
  className = "mt-1",
  emptyLabel,
  onToggle,
}: SessionTreeProps) {
  if (nodes.length === 0 && !emptyLabel) return null;

  return (
    <div className={className}>
      <div
        className="flex cursor-pointer items-center gap-1 px-2.5 pb-1"
        onClick={onToggle}
      >
        <span className="min-w-0 truncate text-[13px] font-semibold uppercase text-foreground-muted">
          {title}
        </span>
        {titleBadge !== undefined && (
          <>
            <span className="text-xs text-foreground-muted">·</span>
            <span className="text-xs text-foreground-muted">{titleBadge}</span>
          </>
        )}
        <span className="text-xs text-foreground-muted">
          <ChevronRight
            className={`h-3.5 w-3.5 shrink-0 transition ${expanded ? "rotate-90" : ""}`}
            aria-hidden="true"
          />
        </span>
      </div>
      {expanded && (
        <SessionTreeContent nodes={nodes} emptyLabel={emptyLabel} />
      )}
    </div>
  );
}

export function SessionTreeContent({
  nodes,
  emptyLabel,
}: SessionTreeContentProps) {
  if (nodes.length === 0 && !emptyLabel) return null;

  return (
    <div className="flex flex-col gap-0.5">
      {nodes.length > 0 ? (
        nodes.map((node) => (
          <SessionTreeRow key={node.id} node={node} depth={0} />
        ))
      ) : (
        <div className="rounded-lg px-2.5 py-2 text-[12px] text-foreground-muted">
          {emptyLabel}
        </div>
      )}
    </div>
  );
}
