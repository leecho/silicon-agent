import { ClipboardList, FileText, Package, Plus, Sparkles } from "lucide-react";
import { DropdownMenu, Tooltip, type DropdownMenuEntry } from "../../../components/ui";
import type { Skill } from "../../../types";
import { skillIcon } from "../../../lib/skillPresentation";
import { useAnchoredMenu } from "./useAnchoredMenu";

const MENU_WIDTH = 184;
const SKILL_MENU_WIDTH = 320;

// [+] 添加：选文件 → onAddFile（外部插入 @路径）；选技能 → onPickSkill（外部插入 chip）。
export function AddMenu({
  pluginNameById = {},
  planMode,
  skills,
  onTogglePlan,
  onPickSkill,
  onAddFile,
}: {
  pluginNameById?: Record<string, string>;
  planMode?: boolean;
  skills: Skill[];
  onTogglePlan?: () => void;
  onPickSkill: (skill: Skill) => void;
  onAddFile: () => void;
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

  return (
    <>
      <Tooltip content="添加文件、计划模式或技能">
        <button
          ref={triggerRef}
          type="button"
          aria-label="添加文件、计划模式或技能"
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
