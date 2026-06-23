import {
  createContext,
  Fragment,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { Check, ChevronRight, type LucideIcon } from "lucide-react";
import { Tooltip } from "./Tooltip";

export interface DropdownMenuPosition {
  x: number;
  y: number;
}

export interface DropdownMenuAnchor {
  bottom: number;
  left: number;
  right: number;
  top: number;
}

type DropdownMenuPlacement = "top" | "bottom";
type DropdownMenuAlign = "start" | "end";
type DropdownMenuRenderState = {
  close: () => void;
};

export type DropdownMenuEntry =
  | {
      danger?: boolean;
      disabled?: boolean;
      children?: DropdownMenuEntry[];
      childrenWidth?: number;
      emptyLabel?: string;
      icon: LucideIcon;
      id: string;
      label: string;
      onSelect?: () => void;
      render?: ReactNode | ((entry: DropdownMenuEntry, state: DropdownMenuRenderState) => ReactNode);
      selected?: boolean;
      tooltip?: string;
      type?: "item";
    }
  | {
      id: string;
      type: "separator";
    }
  | {
      id: string;
      render: ReactNode | ((entry: DropdownMenuEntry, state: DropdownMenuRenderState) => ReactNode);
      type: "custom";
    };

const VIEWPORT_PADDING = 8;
const DEFAULT_OFFSET = 6;
const DEFAULT_MAX_HEIGHT = 360;
const SEARCH_THRESHOLD = 8;
const DropdownMenuCloseContext = createContext<(() => void) | null>(null);

function searchableEntries(entries: DropdownMenuEntry[]) {
  return entries.filter((entry) => entry.type !== "separator" && entry.type !== "custom");
}

function shouldShowSearch(entries: DropdownMenuEntry[]) {
  return searchableEntries(entries).length >= SEARCH_THRESHOLD;
}

function filterDropdownEntries(entries: DropdownMenuEntry[], query: string) {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  if (!normalizedQuery) return entries;
  return entries.filter((entry) => {
    if (entry.type === "separator" || entry.type === "custom") return false;
    const haystack = [entry.label, entry.tooltip, entry.id]
      .filter(Boolean)
      .join(" ")
      .toLocaleLowerCase();
    return haystack.includes(normalizedQuery);
  });
}

export function DropdownMenu({
  align = "start",
  anchorElement,
  anchorRect,
  children,
  items,
  offset = DEFAULT_OFFSET,
  onClose,
  onMouseEnter,
  onMouseLeave,
  placement = "bottom",
  position,
  renderItem,
  width = 184,
}: {
  align?: DropdownMenuAlign;
  anchorElement?: HTMLElement | null;
  anchorRect?: DropdownMenuAnchor | null;
  children?: ReactNode;
  items?: DropdownMenuEntry[];
  offset?: number;
  onClose?: () => void;
  onMouseEnter?: () => void;
  onMouseLeave?: () => void;
  placement?: DropdownMenuPlacement;
  position?: DropdownMenuPosition;
  renderItem?: (entry: DropdownMenuEntry, state: DropdownMenuRenderState) => ReactNode;
  width?: number;
}) {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const [resolvedPosition, setResolvedPosition] = useState<DropdownMenuPosition>(
    position ?? { x: 0, y: 0 },
  );
  const [maxHeight, setMaxHeight] = useState(DEFAULT_MAX_HEIGHT);
  const [query, setQuery] = useState("");

  useLayoutEffect(() => {
    const nextMaxHeight = Math.min(
      DEFAULT_MAX_HEIGHT,
      Math.max(96, window.innerHeight - VIEWPORT_PADDING * 2),
    );
    setMaxHeight(nextMaxHeight);

    if (!anchorRect) {
      if (position) setResolvedPosition(position);
      return;
    }

    const menu = menuRef.current;
    const menuHeight = menu?.offsetHeight ?? 0;
    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;
    const spaceAbove = anchorRect.top - VIEWPORT_PADDING;
    const spaceBelow = viewportHeight - anchorRect.bottom - VIEWPORT_PADDING;
    const shouldPlaceTop =
      placement === "top"
        ? menuHeight <= spaceAbove || spaceAbove >= spaceBelow
        : menuHeight > spaceBelow && spaceAbove > spaceBelow;

    const rawLeft = align === "end" ? anchorRect.right - width : anchorRect.left;
    const left = Math.max(
      VIEWPORT_PADDING,
      Math.min(rawLeft, viewportWidth - width - VIEWPORT_PADDING),
    );
    const rawTop = shouldPlaceTop
      ? anchorRect.top - menuHeight - offset
      : anchorRect.bottom + offset;
    const maxTop = Math.max(VIEWPORT_PADDING, viewportHeight - menuHeight - VIEWPORT_PADDING);
    const top = Math.max(VIEWPORT_PADDING, Math.min(rawTop, maxTop));

    setResolvedPosition({ x: left, y: top });
  }, [align, anchorRect, children, offset, placement, position, width]);

  useEffect(() => {
    if (!anchorRect) return;
    const menu = menuRef.current;
    if (!menu || typeof ResizeObserver === "undefined") return;

    const observer = new ResizeObserver(() => {
      const menuHeight = menu.offsetHeight;
      const viewportHeight = window.innerHeight;
      setMaxHeight(
        Math.min(DEFAULT_MAX_HEIGHT, Math.max(96, viewportHeight - VIEWPORT_PADDING * 2)),
      );
      const spaceAbove = anchorRect.top - VIEWPORT_PADDING;
      const spaceBelow = viewportHeight - anchorRect.bottom - VIEWPORT_PADDING;
      const shouldPlaceTop =
        placement === "top"
          ? menuHeight <= spaceAbove || spaceAbove >= spaceBelow
          : menuHeight > spaceBelow && spaceAbove > spaceBelow;
      const rawTop = shouldPlaceTop
        ? anchorRect.top - menuHeight - offset
        : anchorRect.bottom + offset;
      const maxTop = Math.max(VIEWPORT_PADDING, viewportHeight - menuHeight - VIEWPORT_PADDING);

      setResolvedPosition((current) => ({
        ...current,
        y: Math.max(VIEWPORT_PADDING, Math.min(rawTop, maxTop)),
      }));
    });
    observer.observe(menu);
    return () => observer.disconnect();
  }, [anchorRect, offset, placement]);

  useEffect(() => {
    if (!onClose) return;
    const closeMenu = onClose;

    function handlePointerDown(event: PointerEvent) {
      const target = event.target as Node;
      if (menuRef.current?.contains(target)) return;
      if (anchorElement?.contains(target)) return;
      if (target instanceof HTMLElement && target.closest("[data-dropdown-menu-portal]")) return;
      closeMenu();
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") closeMenu();
    }

    document.addEventListener("pointerdown", handlePointerDown, true);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown, true);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [anchorElement, onClose]);

  const closeMenu = onClose ?? (() => {});
  const showSearch = items ? shouldShowSearch(items) : false;
  const filteredItems = useMemo(
    () => (items ? filterDropdownEntries(items, query) : undefined),
    [items, query],
  );
  const menuContent = items ? (
    <>
      {showSearch && <DropdownMenuSearch value={query} onChange={setQuery} />}
      <div className="min-h-0 overflow-y-auto">
        {filteredItems && filteredItems.length > 0 ? (
          filteredItems.map((entry) =>
            renderDropdownMenuEntry(entry, { close: closeMenu }, renderItem),
          )
        ) : (
          <DropdownMenuEmpty label={query ? "没有匹配项" : "暂无选项"} />
        )}
      </div>
    </>
  ) : (
    children
  );

  const menu = (
    <DropdownMenuCloseContext.Provider value={onClose ?? null}>
      <div
        ref={menuRef}
        data-dropdown-menu-portal="true"
        className="fixed z-[80] flex flex-col rounded-sm border border-border bg-popover p-1 text-popover-foreground shadow-[0_10px_24px_rgba(15,23,42,0.14)]"
        style={{ left: resolvedPosition.x, maxHeight, overflowY: "hidden", top: resolvedPosition.y, width }}
        onClick={(event) => event.stopPropagation()}
        onMouseEnter={onMouseEnter}
        onMouseLeave={onMouseLeave}
      >
        {menuContent}
      </div>
    </DropdownMenuCloseContext.Provider>
  );

  return createPortal(menu, document.body);
}

function resolveDropdownMenuRender(
  entry: DropdownMenuEntry,
  state: DropdownMenuRenderState,
  render: ReactNode | ((entry: DropdownMenuEntry, state: DropdownMenuRenderState) => ReactNode),
) {
  return typeof render === "function" ? render(entry, state) : render;
}

function renderDropdownMenuEntry(
  entry: DropdownMenuEntry,
  state: DropdownMenuRenderState,
  renderItem?: (entry: DropdownMenuEntry, state: DropdownMenuRenderState) => ReactNode,
) {
  return (
    <DropdownMenuEntryView
      key={entry.id}
      entry={entry}
      renderItem={renderItem}
      state={state}
    />
  );
}

function DropdownMenuEntryView({
  entry,
  renderItem,
  state,
}: {
  entry: DropdownMenuEntry;
  renderItem?: (entry: DropdownMenuEntry, state: DropdownMenuRenderState) => ReactNode;
  state: DropdownMenuRenderState;
}) {
  const [subOpen, setSubOpen] = useState(false);
  const anchorRef = useRef<HTMLDivElement | null>(null);
  const closeTimerRef = useRef<number | null>(null);
  const custom = renderItem?.(entry, state);

  useEffect(() => {
    return () => {
      if (closeTimerRef.current !== null) window.clearTimeout(closeTimerRef.current);
    };
  }, []);

  const openSubmenu = () => {
    if (closeTimerRef.current !== null) {
      window.clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
    setSubOpen(true);
  };
  const scheduleCloseSubmenu = () => {
    if (closeTimerRef.current !== null) window.clearTimeout(closeTimerRef.current);
    closeTimerRef.current = window.setTimeout(() => setSubOpen(false), 120);
  };

  if (custom != null) {
    return <Fragment>{custom}</Fragment>;
  }

  if (entry.type === "separator") {
    return <DropdownMenuSeparator />;
  }

  if (entry.type === "custom") {
    return (
      <Fragment>
        {resolveDropdownMenuRender(entry, state, entry.render)}
      </Fragment>
    );
  }

  const hasChildren = Boolean(entry.children);
  const children = entry.children ?? [];
  return (
    <div
      ref={anchorRef}
      className="relative"
      onMouseEnter={openSubmenu}
      onMouseLeave={scheduleCloseSubmenu}
    >
      <DropdownMenuItem
        danger={entry.danger}
        disabled={entry.disabled}
        hasChildren={hasChildren}
        icon={entry.icon}
        label={entry.label}
        selected={entry.selected}
        tooltip={entry.tooltip}
        content={entry.render ? resolveDropdownMenuRender(entry, state, entry.render) : undefined}
        onClick={hasChildren ? undefined : entry.onSelect}
      />
      {hasChildren && subOpen && (
        <DropdownSubMenu
          anchorElement={anchorRef.current}
          emptyLabel={entry.emptyLabel}
          entries={children}
          onMouseEnter={openSubmenu}
          onMouseLeave={scheduleCloseSubmenu}
          renderItem={renderItem}
          state={state}
          top={0}
          width={entry.childrenWidth}
        />
      )}
    </div>
  );
}

export function DropdownMenuItem({
  children,
  content,
  danger,
  disabled,
  hasChildren,
  icon: Icon,
  label,
  onClick,
  onMouseEnter,
  onMouseLeave,
  selected,
  tooltip,
}: {
  children?: ReactNode;
  content?: ReactNode;
  danger?: boolean;
  disabled?: boolean;
  hasChildren?: boolean;
  icon: LucideIcon;
  label: string;
  onClick?: () => void;
  onMouseEnter?: () => void;
  onMouseLeave?: () => void;
  selected?: boolean;
  tooltip?: string;
}) {
  const closeMenu = useContext(DropdownMenuCloseContext);
  const customContent = content ?? children;

  return (
    <Tooltip content={tooltip} disabled={!tooltip}>
      <button
        className={`flex w-full gap-2.5 rounded-sm px-2.5 text-left text-[13px] h-8 items-center transition ${
          selected
            ? "bg-primary text-white"
            : danger
              ? "text-destructive hover:bg-destructive/10"
              : "text-popover-foreground hover:bg-accent hover:text-accent-foreground"
        } disabled:cursor-not-allowed disabled:opacity-45`}
        disabled={disabled}
        type="button"
        onClick={() => {
          onClick?.();
          if (onClick) closeMenu?.();
        }}
        onMouseEnter={onMouseEnter}
        onMouseLeave={onMouseLeave}
      >
        <Icon className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
        {customContent ? (
          customContent
        ) : (
          <span className="truncate text-[13px] text-current">{label}</span>
        )}
        {(hasChildren || selected) && (
          <span className="shrink-0 flex flex-1 justify-end">
            {hasChildren ? (
              <ChevronRight className="h-3.5 w-3.5 shrink-0 text-current " aria-hidden="true" />
            ) : (
              <Check className="h-3.5 w-3.5 shrink-0 text-current" aria-hidden="true" />
            )}
          </span>
        )}
      </button>
    </Tooltip>
  );
}

export function DropdownMenuSeparator() {
  return <div className="my-1 border-t border-border" />;
}

function DropdownMenuSearch({
  onChange,
  value,
}: {
  onChange: (value: string) => void;
  value: string;
}) {
  return (
    <div className="mb-1 px-1">
      <input
        className="h-8 w-full rounded-sm border border-border bg-background px-2 text-[13px] text-foreground outline-none placeholder:text-foreground-muted focus:ring-1 focus:ring-ring"
        placeholder="搜索"
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    </div>
  );
}

function DropdownMenuEmpty({ label }: { label: string }) {
  return (
    <div className="px-2.5 py-1.5 text-[12px] text-foreground-muted">
      {label}
    </div>
  );
}

export function DropdownSubMenu({
  anchorElement,
  children,
  emptyLabel,
  entries,
  left,
  onMouseEnter,
  onMouseLeave,
  renderItem,
  state,
  top = 0,
  width = 184,
}: {
  anchorElement?: HTMLElement | null;
  children?: ReactNode;
  emptyLabel?: string;
  entries?: DropdownMenuEntry[];
  left?: number;
  onMouseEnter?: () => void;
  onMouseLeave?: () => void;
  renderItem?: (entry: DropdownMenuEntry, state: DropdownMenuRenderState) => ReactNode;
  state?: DropdownMenuRenderState;
  top?: number;
  width?: number;
}) {
  const submenuRef = useRef<HTMLDivElement | null>(null);
  const [resolvedPosition, setResolvedPosition] = useState<DropdownMenuPosition>({
    x: left ?? 0,
    y: top,
  });
  const [maxHeight, setMaxHeight] = useState(DEFAULT_MAX_HEIGHT);
  const [query, setQuery] = useState("");
  const showSearch = entries ? shouldShowSearch(entries) : false;
  const filteredEntries = useMemo(
    () => (entries ? filterDropdownEntries(entries, query) : undefined),
    [entries, query],
  );

  useLayoutEffect(() => {
    const menu = submenuRef.current;
    const anchor = anchorElement ?? menu?.parentElement;
    if (!menu || !anchor) return;

    const anchorRect = anchor.getBoundingClientRect();
    const menuHeight = menu.offsetHeight;
    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;

    const rightSideLeft = anchorRect.right + (left ?? 0);
    const leftSideLeft = anchorRect.left - width + 4;
    const openLeft = rightSideLeft + width > viewportWidth - VIEWPORT_PADDING;
    const nextLeft = openLeft
      ? Math.max(VIEWPORT_PADDING, leftSideLeft)
      : Math.min(rightSideLeft, viewportWidth - width - VIEWPORT_PADDING);

    const preferredTop = anchorRect.top + top;
    const maxAbsoluteTop = viewportHeight - VIEWPORT_PADDING - menuHeight;
    const nextTop = Math.max(VIEWPORT_PADDING, Math.min(preferredTop, maxAbsoluteTop));

    setResolvedPosition({ x: nextLeft, y: nextTop });
    setMaxHeight(
      Math.min(DEFAULT_MAX_HEIGHT, Math.max(96, viewportHeight - VIEWPORT_PADDING * 2)),
    );
  }, [anchorElement, children, filteredEntries, left, top, width]);

  const submenu = (
    <div
      ref={submenuRef}
      data-dropdown-menu-portal="true"
      className="fixed z-[999999] flex flex-col rounded-sm border border-border bg-popover p-1 text-popover-foreground shadow-[0_10px_24px_rgba(15,23,42,0.14)]"
      style={{
        left: resolvedPosition.x,
        maxHeight,
        overflowY: "hidden",
        top: resolvedPosition.y,
        width,
      }}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
    >
      {showSearch && <DropdownMenuSearch value={query} onChange={setQuery} />}
      <div className="min-h-0 overflow-y-auto">
        {filteredEntries && state ? (
          filteredEntries.length > 0 ? (
            filteredEntries.map((entry) =>
              renderDropdownMenuEntry(entry, state, renderItem),
            )
          ) : (
            <DropdownMenuEmpty label={query ? "没有匹配项" : (emptyLabel ?? "暂无选项")} />
          )
        ) : (
          children
        )}
      </div>
    </div>
  );

  return createPortal(submenu, document.body);
}
