import { Bot, ChevronDown, Package, Sparkles, Users } from "lucide-react";
import { DropdownMenu, Tooltip, type DropdownMenuEntry } from "../../../components/ui";
import type { ExpertSummary, Team } from "../../../types";
import { useAnchoredMenu } from "./useAnchoredMenu";

// 会话「角色槽」选择（互斥）：专家（散装 agent，作主对话人设）/ 团队（lead+成员派活）。
// 下拉为二级菜单：一级选模式，二级列出可用专家或团队；选中由 (kind, id) 判定。
export function TeamPicker({
  value,
  teams,
  agents,
  onPick,
}: {
  value: { kind: string; id: string };
  teams: Team[];
  agents: ExpertSummary[];
  onPick: (kind: string, id: string) => void;
}) {
  const { anchorRect, open, triggerRef, toggle, close } = useAnchoredMenu();

  const selectedTeam =
    value.kind === "team" ? teams.find((t) => t.id === value.id) : undefined;
  const selectedExpert =
    value.kind === "expert" ? agents.find((a) => a.name === value.id) : undefined;
  const label =
    selectedTeam?.displayName ??
    selectedExpert?.displayName ??
    selectedExpert?.name ??
    "默认";
  const TriggerIcon = selectedTeam ? Users : selectedExpert ? Bot : Sparkles;

  const agentItems: DropdownMenuEntry[] = agents.map((a) => ({
    id: `agent:${a.name}`,
    icon: Bot,
    label: a.displayName || a.name,
    selected: value.kind === "expert" && value.id === a.name,
    onSelect: () => onPick("expert", a.name),
  }));

  const teamItems: DropdownMenuEntry[] = teams.map((t) => ({
    id: `team:${t.id}`,
    icon: Users,
    label: t.displayName,
    tooltip: t.description,
    selected: value.kind === "team" && value.id === t.id,
    onSelect: () => onPick("team", t.id),
  }));

  const items: DropdownMenuEntry[] = [
    {
      id: "__default__",
      icon: Sparkles,
      label: "默认",
      tooltip: "清空当前选择，由助手按默认方式处理。",
      selected: value.kind === "" || value.kind === "free",
      onSelect: () => onPick("", ""),
    },
    {
      id: "__agents__",
      icon: Bot,
      label: "专家",
      tooltip: "选择一个专家作为当前对话身份。",
      selected: value.kind === "expert",
      children: agentItems,
      childrenWidth: 216,
      emptyLabel: "暂无可用专家",
    },
    {
      id: "__teams__",
      icon: Users,
      label: "团队",
      tooltip: "选择一个团队，由主理人安排成员协作。",
      selected: value.kind === "team",
      children: teamItems,
      childrenWidth: 216,
      emptyLabel: "暂无可用团队",
    },
  ];
  if (teams.length === 0 && agents.length === 0) {
    items.push({
      id: "__hint__",
      icon: Package,
      label: "还没有专家或团队，去「专家」或「团队」页创建",
      disabled: true,
    });
  }

  return (
    <>
      <Tooltip
        content={
          teams.length > 0 || agents.length > 0
            ? "选择一个专家或团队作为当前对话角色"
            : "你还没有专家或团队，去「专家」或「团队」页创建"
        }
      >
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
          width={168}
          items={items}
        />
      )}
    </>
  );
}
