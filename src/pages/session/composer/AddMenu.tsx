import { BookMarked, Bot, Check, ClipboardList, FileText, Package, Plus, Sparkles, Users } from "lucide-react";
import { DropdownMenu, Tooltip, type DropdownMenuEntry } from "../../../components/ui";
import type { ExpertSummary, KnowledgeBase, Skill, Team } from "../../../types";
import { skillIcon } from "../../../lib/skillPresentation";
import { useAnchoredMenu } from "./useAnchoredMenu";

const MENU_WIDTH = 184;
const SKILL_MENU_WIDTH = 320;
const KB_MENU_WIDTH = 200;

// [+] 添加：选文件 → onAddFile（外部插入 @路径）；选技能 → onPickSkill（外部插入 chip）；
// 资料库子菜单：点击挂/卸到当前会话（选中打勾，chip 显示在 + 号旁）。
export function AddMenu({
  pluginNameById,
  planMode,
  skills,
  knowledgeBases,
  mountedKbIds,
  onToggleKb,
  onTogglePlan,
  onPickSkill,
  onAddFile,
  roleValue,
  teams,
  roleExperts,
  onPickRole,
}: {
  pluginNameById: Record<string, string>;
  planMode?: boolean;
  skills: Skill[];
  knowledgeBases: KnowledgeBase[];
  mountedKbIds: string[];
  onToggleKb: (id: string) => void;
  onTogglePlan?: () => void;
  onPickSkill: (skill: Skill) => void;
  onAddFile: () => void;
  /** 当前会话角色（kind: ""/"expert"/"team"）；仅当传入 onPickRole 时渲染角色区。 */
  roleValue?: { kind: string; id: string };
  /** 可选团队（角色「团队」子菜单）。 */
  teams?: Team[];
  /** 可选散装专家（角色「专家」子菜单）。 */
  roleExperts?: ExpertSummary[];
  /** 选择角色（kind 空串 = 默认/自由模式）；缺省则不渲染角色区。 */
  onPickRole?: (kind: string, id: string) => void;
}) {
  const { anchorRect, open, triggerRef, toggle, close } = useAnchoredMenu();
  const items: DropdownMenuEntry[] = [
    {
      icon: FileText,
      id: "add-file",
      label: "添加文件",
      onSelect: () => {
        close();
        onAddFile();
      },
    },
  ];

  items.push({
    children: buildSkillMenuEntries(skills, pluginNameById, (skill) => {
      close();
      onPickSkill(skill);
    }),
    childrenWidth: SKILL_MENU_WIDTH,
    emptyLabel: "暂无启用的技能",
    icon: Sparkles,
    id: "add-skill",
    label: "添加技能",
  });

  // 资料库子菜单：点击挂/卸（选中打勾，不关闭菜单可连续选）。
  items.push({
    icon: BookMarked,
    id: "knowledge",
    label: "资料库",
    children: knowledgeBases.map(
      (kb): DropdownMenuEntry => {
        const on = mountedKbIds.includes(kb.id);
        return {
          icon: BookMarked,
          id: `kb:${kb.id}`,
          label: kb.name,
          onSelect: () => onToggleKb(kb.id),
          render: (
            <span className="flex min-w-0 flex-1 items-center justify-between gap-3">
              <span className="min-w-0 truncate text-[13px] text-current">{kb.name}</span>
              {on ? <Check className="h-3.5 w-3.5 shrink-0 text-primary" aria-hidden="true" /> : null}
            </span>
          ),
        };
      },
    ),
    childrenWidth: KB_MENU_WIDTH,
    emptyLabel: "暂无资料库（去「资料库」页创建）",
  });

  if (onTogglePlan) {
    items.push(
      { id: "plan-mode-separator", type: "separator" },
      {
        icon: ClipboardList,
        id: "plan-mode",
        label: "计划模式",
        onSelect: () => {
          close();
          onTogglePlan();
        },
        render: (
          <span className="flex min-w-0 flex-1 items-center justify-between gap-3">
            <span className="truncate text-[13px] text-current">计划模式</span>
            <PlanModeSwitch enabled={Boolean(planMode)} />
          </span>
        ),
      }
    );
  }

  // 角色选择（默认 / 专家 / 团队）：逻辑搬自原独立 TeamPicker。仅传入 onPickRole 时渲染。
  if (onPickRole) {
    const role = roleValue ?? { kind: "", id: "" };
    const expertList = roleExperts ?? [];
    const teamList = teams ?? [];
    const agentItems: DropdownMenuEntry[] = expertList.map((a) => ({
      id: `role-agent:${a.name}`,
      icon: Bot,
      label: a.displayName || a.name,
      selected: role.kind === "expert" && role.id === a.name,
      onSelect: () => {
        close();
        onPickRole("expert", a.name);
      },
    }));
    const teamItems: DropdownMenuEntry[] = teamList.map((t) => ({
      id: `role-team:${t.id}`,
      icon: Users,
      label: t.displayName,
      tooltip: t.description,
      selected: role.kind === "team" && role.id === t.id,
      onSelect: () => {
        close();
        onPickRole("team", t.id);
      },
    }));
    items.push(
      { id: "role-separator", type: "separator" },
      {
        id: "role-agents",
        icon: Bot,
        label: "专家",
        tooltip: "选择一个专家作为当前对话身份。",
        selected: role.kind === "expert",
        children: agentItems,
        childrenWidth: 216,
        emptyLabel: "暂无可用专家",
      },
      {
        id: "role-teams",
        icon: Users,
        label: "团队",
        tooltip: "选择一个团队，由主理人安排成员协作。",
        selected: role.kind === "team",
        children: teamItems,
        childrenWidth: 216,
        emptyLabel: "暂无可用团队",
      },
    );
  }

  return (
    <>
      <Tooltip content="添加文件、资料库、技能、计划模式或角色">
        <button
          ref={triggerRef}
          type="button"
          aria-label="添加文件、资料库、技能、计划模式或角色"
          className="grid h-7 w-7 place-items-center rounded-full border border-border text-foreground-secondary hover:bg-accent"
          onClick={(e) => {
            e.stopPropagation();
            toggle();
          }}
        >
          <Plus className="h-4 w-4" aria-hidden="true" />
        </button>
      </Tooltip>
      {open && (
        <DropdownMenu
          anchorElement={triggerRef.current}
          anchorRect={anchorRect}
          onClose={close}
          placement="top"
          width={MENU_WIDTH}
          items={items}
        />
      )}
    </>
  );
}

function PlanModeSwitch({ enabled }: { enabled: boolean }) {
  return (
    <span
      aria-hidden="true"
      className={`relative h-5 w-9 shrink-0 rounded-full transition ${
        enabled ? "bg-primary" : "bg-muted"
      }`}
    >
      <span
        className={`absolute top-0.5 h-4 w-4 rounded-full bg-white transition ${
          enabled ? "left-4" : "left-0.5"
        }`}
      />
    </span>
  );
}

function buildSkillMenuEntries(
  skills: Skill[],
  pluginNameById: Record<string, string>,
  onPickSkill: (skill: Skill) => void,
): DropdownMenuEntry[] {
  const looseSkills: Skill[] = [];
  const pluginSkillsById = new Map<string, Skill[]>();

  for (const skill of skills) {
    if (!skill.pluginId) {
      looseSkills.push(skill);
      continue;
    }
    const group = pluginSkillsById.get(skill.pluginId) ?? [];
    group.push(skill);
    pluginSkillsById.set(skill.pluginId, group);
  }

  const entries: DropdownMenuEntry[] = [];
  const pluginIds = Array.from(pluginSkillsById.keys()).sort((a, b) =>
    (pluginNameById[a] ?? a).localeCompare(pluginNameById[b] ?? b, "zh-Hans-CN"),
  );

  for (const pluginId of pluginIds) {
    entries.push({
      id: `plugin:${pluginId}`,
      type: "custom",
      render: (
        <div className="flex h-7 items-center gap-1.5 px-2.5 pt-1 text-[11px] font-medium text-foreground-muted">
          <Package className="h-3 w-3 shrink-0" aria-hidden="true" />
          <span className="min-w-0 truncate">{pluginNameById[pluginId] ?? pluginId}</span>
        </div>
      ),
    });

    for (const skill of pluginSkillsById.get(pluginId) ?? []) {
      entries.push(buildSkillEntry(skill, onPickSkill));
    }
  }

  if (pluginIds.length > 0 && looseSkills.length > 0) {
    entries.push({ id: "loose-skills-separator", type: "separator" });
  }

  for (const skill of looseSkills) {
    entries.push(buildSkillEntry(skill, onPickSkill));
  }

  return entries;
}

function buildSkillEntry(s: Skill, onPickSkill: (skill: Skill) => void): DropdownMenuEntry {
  return {
    icon: skillIcon(s),
    id: s.id,
    label: s.name,
    onSelect: () => onPickSkill(s),
    render: (entry) => {
      const entryLabel = "label" in entry ? entry.label : s.name;
      return (
        <span className="min-w-0 flex flex-row gap-1">
          <span className="block truncate text-[13px] text-current">{entryLabel}</span>
          {s.description && (
            <span className="mt-0.5 block flex-1 truncate text-[11px] text-foreground-muted">
              {s.description}
            </span>
          )}
        </span>
      );
    },
  };
}
