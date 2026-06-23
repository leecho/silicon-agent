import { ChevronDown, ShieldCheck, ShieldPlus, ShieldUser, type LucideIcon } from "lucide-react";
import { DropdownMenu, Tooltip } from "../../../components/ui";
import type { PermissionMode } from "../../../types";
import { useAnchoredMenu } from "./useAnchoredMenu";

const PERMISSION_MODES: {
  value: PermissionMode;
  label: string;
  detail: string;
  icon: LucideIcon;
}[] = [
  {
    value: "manual",
    label: "手动审批",
    detail: "每次工具操作都需要确认，适合高风险任务。",
    icon: ShieldUser,
  },
  {
    value: "auto",
    label: "自动审批",
    detail: "低风险操作自动通过，敏感操作仍需确认。",
    icon: ShieldPlus,
  },
  {
    value: "full",
    label: "完全授权",
    detail: "允许 Agent 自主执行工具操作，适合无人值守任务。",
    icon: ShieldCheck,
  },
];

// 会话权限模式紧凑下拉：未覆盖时直接选中全局默认模式。
export function PermissionPicker({
  value,
  globalDefault,
  onChange,
}: {
  value: PermissionMode | null;
  globalDefault: PermissionMode;
  onChange: (mode: PermissionMode | null) => void;
}) {
  const { anchorRect, open, triggerRef, toggle, close } = useAnchoredMenu();
  const effectiveValue = value ?? globalDefault;
  const selectedMode = PERMISSION_MODES.find((m) => m.value === effectiveValue);
  const label = selectedMode?.label ?? effectiveValue;
  const TriggerIcon = selectedMode?.icon ?? ShieldCheck;

  return (
    <>
      <Tooltip content="权限模式">
        <button
          ref={triggerRef}
          type="button"
          className="flex items-center gap-1 rounded-md px-2 py-1.5 text-xs text-foreground-secondary hover:bg-accent"
          onClick={(e) => {
            e.stopPropagation();
            toggle();
          }}
        >
          <TriggerIcon className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <span className="max-w-[120px] truncate">{label}</span>
          <ChevronDown className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
        </button>
      </Tooltip>
      {open && (
        <DropdownMenu
          align="end"
          anchorElement={triggerRef.current}
          anchorRect={anchorRect}
          onClose={close}
          placement="top"
          items={PERMISSION_MODES.map((m) => ({
            icon: m.icon,
            id: m.value,
            label: m.label,
            tooltip: m.detail,
            onSelect: () => onChange(m.value === globalDefault ? null : m.value),
            // render: (entry) => {
            //   const selected = m.value === effectiveValue;
            //   const entryLabel = "label" in entry ? entry.label : m.label;
            //   return (
            //     <span className="min-w-0 flex gap-1 flex-row">
            //       <span className="block truncate text-[13px] text-current">{entryLabel}</span>
            //       <span className={`mt-0.5 block flex-1 truncate text-[11px] ${selected ? "text-white/80" : "text-foreground-muted"}`}>
            //         {m.detail}
            //       </span>
            //     </span>
            //   );
            // },
            selected: m.value === effectiveValue,
          }))}
        />
      )}
    </>
  );
}
