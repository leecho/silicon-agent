import { ChevronDown, Search } from "lucide-react";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Tooltip } from "./Tooltip";
import { joinClasses } from "./utils";

export type SelectOption = {
  description?: string;
  disabled?: boolean;
  group?: string;
  label: string;
  searchText?: string;
  value: string;
};

export function Select({
  className,
  disabled = false,
  onChange,
  options,
  renderOption,
  searchable = false,
  searchPlaceholder = "筛选选项",
  tooltip,
  value
}: {
  className?: string;
  disabled?: boolean;
  onChange: (value: string) => void;
  options: SelectOption[];
  renderOption?: (option: SelectOption, state: { selected: boolean }) => ReactNode;
  searchable?: boolean;
  searchPlaceholder?: string;
  tooltip?: string;
  value: string;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [menuRect, setMenuRect] = useState({
    bottom: undefined as number | undefined,
    left: 0,
    maxHeight: 288,
    maxWidth: 480,
    placement: "bottom" as "bottom" | "top",
    top: undefined as number | undefined,
    width: 0
  });
  const rootRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const selected = options.find((option) => option.value === value);
  const portalTarget = rootRef.current?.closest(".theme-light") ?? document.body;
  const hasOptionDescriptions = options.some((option) => Boolean(option.description));
  const filteredOptions = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!searchable || !q) return options;
    return options.filter((option) =>
      [option.label, option.description, option.group, option.value, option.searchText]
        .filter((text): text is string => Boolean(text))
        .some((text) => text.toLowerCase().includes(q))
    );
  }, [options, query, searchable]);

  const updateMenuRect = useCallback(() => {
    const rect = rootRef.current?.getBoundingClientRect();
    if (!rect) return;
    const viewportGap = 12;
    const menuGap = 6;
    const preferredHeight = searchable ? 336 : 288;
    const minUsefulHeight = 160;
    const availableBelow = window.innerHeight - rect.bottom - viewportGap - menuGap;
    const availableAbove = rect.top - viewportGap - menuGap;
    const placement = availableBelow >= minUsefulHeight || availableBelow >= availableAbove ? "bottom" : "top";
    const availableHeight = placement === "bottom" ? availableBelow : availableAbove;
    const maxHeight = Math.max(120, Math.min(preferredHeight, availableHeight));
    const viewportWidth = window.innerWidth - viewportGap * 2;
    const preferredWidth = rect.width;
    const width = Math.min(preferredWidth, viewportWidth);
    const left = Math.min(rect.left, window.innerWidth - viewportGap - width);
    setMenuRect({
      bottom: placement === "top" ? window.innerHeight - rect.top + menuGap : undefined,
      left: Math.max(viewportGap, left),
      maxHeight,
      maxWidth: viewportWidth,
      placement,
      top: placement === "bottom" ? rect.bottom + menuGap : undefined,
      width
    });
  }, [hasOptionDescriptions, searchable]);

  useEffect(() => {
    if (!open) return;
    updateMenuRect();

    function handlePointerDown(event: PointerEvent) {
      if (rootRef.current?.contains(event.target as Node)) return;
      if (menuRef.current?.contains(event.target as Node)) return;
      setOpen(false);
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }

    function handlePositionChange() {
      updateMenuRect();
    }

    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    window.addEventListener("resize", handlePositionChange);
    window.addEventListener("scroll", handlePositionChange, true);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("resize", handlePositionChange);
      window.removeEventListener("scroll", handlePositionChange, true);
    };
  }, [open, updateMenuRect]);

  function choose(nextValue: string) {
    onChange(nextValue);
    setOpen(false);
    setQuery("");
  }

  return (
    <div className="relative min-w-0" ref={rootRef}>
      <Tooltip content={tooltip} disabled={!tooltip}>
        <button
          aria-expanded={open}
          className={joinClasses(
            "group flex h-9 min-w-0 text-[13px] items-center justify-between gap-2 rounded-lg border border-input bg-background px-3 text-left text-foreground transition focus:border-ring disabled:cursor-not-allowed disabled:opacity-50",
            className
          )}
          disabled={disabled}
          type="button"
          onClick={() => {
            updateMenuRect();
            setOpen((current) => !current);
          }}
        >
          <span className="min-w-0 flex-1 truncate leading-none">{selected?.label ?? (value || "请选择")}</span>
          <ChevronDown className={joinClasses("block h-4 w-4 shrink-0 self-center text-foreground-muted transition group-hover:text-foreground", open && "rotate-180 text-foreground")} aria-hidden="true" />
        </button>
      </Tooltip>
      {open && createPortal(
        <div
          className="fixed z-[100] overflow-y-auto overflow-x-hidden rounded-lg border border-border bg-popover p-1 text-popover-foreground shadow-2xl"
          ref={menuRef}
          role="listbox"
          style={{
            bottom: menuRect.bottom,
            left: menuRect.left,
            maxHeight: menuRect.maxHeight,
            maxWidth: menuRect.maxWidth,
            minWidth: menuRect.width,
            top: menuRect.top,
            width: menuRect.width
          }}
        >
          <div className="grid min-h-0 gap-1">
            {searchable && (
              <label className="sticky top-0 z-10 mb-1 flex h-9 min-w-0 items-center gap-2 rounded-md border border-input bg-background px-2 text-foreground">
                <Search className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
                <input
                  className="text-sm min-w-0 flex-1 bg-transparent outline-none placeholder:text-foreground-muted"
                  value={query}
                  placeholder={searchPlaceholder}
                  onChange={(event) => setQuery(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape") setOpen(false);
                  }}
                />
              </label>
            )}
            {filteredOptions.map((option, index) => {
              const active = option.value === value;
              const previous = filteredOptions[index - 1];
              const showGroup = option.group && option.group !== previous?.group;
              return (
                <div key={option.value} className="min-w-0">
                  {showGroup && (
                    <div
                      className={joinClasses(
                        "px-2.5 pb-1 pt-2 text-[11px] font-medium text-foreground-muted",
                        index > 0 && "mt-1 border-t border-border"
                      )}
                    >
                      {option.group}
                    </div>
                  )}
                  <button
                    aria-selected={active}
                    className={joinClasses(
                      "text-sm flex w-full min-w-0 items-center rounded-md px-2.5 py-2 text-left transition disabled:cursor-not-allowed disabled:opacity-40",
                      active ? "bg-primary text-white" : "text-foreground-secondary hover:bg-accent hover:text-accent-foreground"
                    )}
                    disabled={option.disabled}
                    role="option"
                    type="button"
                    onClick={() => choose(option.value)}
                  >
                    <span className="min-w-0 flex-1 overflow-hidden">
                      {renderOption ? renderOption(option, { selected: active }) : <span className="block truncate">{option.label}</span>}
                    </span>
                  </button>
                </div>
              );
            })}
            {filteredOptions.length === 0 && (
              <div className="px-2.5 py-2 font-[family-name:var(--app-font-family)] text-[13px] leading-[1.45] text-foreground-muted">
                暂无匹配选项
              </div>
            )}
          </div>
        </div>,
        portalTarget
      )}
    </div>
  );
}
