import { Bot, ChevronDown, FolderKanban, FolderLock, FolderMinus, FolderPlus } from "lucide-react";
import {
  DropdownMenu,
  Tooltip,
  type DropdownMenuEntry,
} from "../../../components/ui";
import { useAnchoredMenu } from "./useAnchoredMenu";
import type { Agent, Project } from "../../../types";

const WS_MENU_WIDTH = 184;

// 工作上下文选择下拉：智能体（带专属工作目录+私有记忆）/ 项目 / 本地目录 + 最近使用 +（可选）清空。
export function WorkspacePicker({
  projects,
  selectedProjectId,
  agents,
  selectedAgentId,
  onPickAgent,
  workspaceName,
  workspacePath,
  onPickProject,
  onPickWorkspace,
  recentWorkspaces,
  onPickRecent,
  onClear,
  locked,
}: {
  projects?: Project[];
  selectedProjectId?: string | null;
  /** 可选智能体（绑定后该会话用其专属工作目录 + 人设 + 私有记忆）。 */
  agents?: Agent[];
  /** 当前会话绑定的持久智能体 id。 */
  selectedAgentId?: string | null;
  /** 选择一个智能体作为当前会话所属实体。 */
  onPickAgent?: (agentId: string) => void;
  workspaceName?: string;
  workspacePath?: string;
  onPickProject?: (projectId: string) => void;
  onPickWorkspace?: () => void;
  recentWorkspaces?: string[];
  onPickRecent?: (path: string) => void;
  /** 提供后菜单出现「清空工作目录」项（仅当前已选目录时显示）。 */
  onClear?: () => void;
  /** 锁定（如项目/智能体上下文）：只读展示上下文 + 锁图标，不可更改。 */
  locked?: boolean;
}) {
  const { anchorRect, open, triggerRef, toggle, close } = useAnchoredMenu();
  const selectedProject = (projects ?? []).find((project) => project.id === selectedProjectId);
  const selectedAgent = (agents ?? []).find((a) => a.id === selectedAgentId);
  const lockedLabel =
    selectedProject?.name ?? selectedAgent?.displayName ?? selectedAgent?.name ?? workspaceName ?? "工作上下文";
  if (locked) {
    return (
      <Tooltip
        content={
          workspacePath
            ? `${selectedProject ? `项目：${selectedProject.name} · ` : selectedAgent ? `智能体：${selectedAgent.displayName || selectedAgent.name} · ` : ""}工作目录（不可更改）：${workspacePath}`
            : selectedProject
              ? `项目：${selectedProject.name}`
              : selectedAgent
                ? `智能体：${selectedAgent.displayName || selectedAgent.name}`
                : "工作上下文（不可更改）"
        }
      >
        <span className="flex min-w-0 max-w-full items-center gap-1 rounded px-2 py-1.5 text-xs text-foreground-muted">
          <FolderLock className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <span className="min-w-0 truncate">{lockedLabel}</span>
        </span>
      </Tooltip>
    );
  }
  const closeAll = () => {
    close();
  };
  const projectItems: DropdownMenuEntry[] = (projects ?? []).map((project) => ({
    icon: FolderKanban,
    id: `project-${project.id}`,
    label: project.name,
    selected: project.id === selectedProjectId,
    tooltip: project.description || project.name,
    onSelect: () => {
      closeAll();
      onPickProject?.(project.id);
    },
  }));
  const recentItems: DropdownMenuEntry[] = (recentWorkspaces ?? []).map(
    (p): DropdownMenuEntry => {
      // 只显示最后一级文件夹名；全路径走 tooltip。
      const leaf = p.split(/[/\\]+/).filter(Boolean).pop() ?? p;
      return {
        id: `recent-${p}`,
        type: "custom",
        render: (_entry, state) => (
          <Tooltip content={p}>
            <button
              type="button"
              className="flex h-8 w-full items-center rounded-[8px] px-2.5 text-left text-[13px] text-foreground transition hover:bg-muted"
              onClick={() => {
                state.close();
                onPickRecent?.(p);
              }}
            >
              <span className="min-w-0 flex-1 truncate">{leaf}</span>
            </button>
          </Tooltip>
        ),
      };
    },
  );
  const directoryItems: DropdownMenuEntry[] = [
    {
      icon: FolderPlus,
      id: "pick-directory",
      label: workspaceName ? "清空并替换目录" : "选择目录",
      onSelect: () => {
        closeAll();
        onPickWorkspace?.();
      },
    },
    // 「最近使用的目录」分组：本地目录三级页内的组标题 + 列表（不再多套一层子菜单）。
    { id: "recent-separator", type: "separator" },
    {
      id: "recent-label",
      type: "custom",
      render: () => (
        <div className="px-2.5 pb-1 pt-1.5 text-[11px] font-medium text-foreground-muted">
          最近使用的目录
        </div>
      ),
    },
    ...(recentItems.length > 0
      ? recentItems
      : [
          {
            id: "recent-empty",
            type: "custom" as const,
            render: () => (
              <div className="px-2.5 py-1.5 text-[12px] text-foreground-muted">暂无最近目录</div>
            ),
          },
        ]),
  ];
  if (onClear && workspaceName) {
    directoryItems.push(
      { id: "clear-separator", type: "separator" },
      {
        icon: FolderMinus,
        id: "clear-directory",
        label: "清空工作目录",
        onSelect: () => {
          closeAll();
          onClear();
        },
      },
    );
  }
  const agentItems: DropdownMenuEntry[] = (agents ?? []).map((agent) => ({
    icon: Bot,
    id: `agent-${agent.id}`,
    label: agent.displayName || agent.name,
    selected: agent.id === selectedAgentId,
    tooltip: agent.profession || agent.name,
    onSelect: () => {
      closeAll();
      onPickAgent?.(agent.id);
    },
  }));
  const menuItems: DropdownMenuEntry[] = [];
  if (onPickAgent) {
    const children: DropdownMenuEntry[] = selectedAgentId
      ? [
          ...agentItems,
        ]
      : agentItems;
    menuItems.push({
      children,
      emptyLabel: "暂无智能体",
      icon: Bot,
      id: "agents",
      label: "智能体",
    });
  }
  if (projects || onPickProject) {
    menuItems.push({
      children: projectItems,
      emptyLabel: "暂无项目",
      icon: FolderKanban,
      id: "projects",
      label: "项目",
    });
  }
  menuItems.push({
    children: directoryItems,
    icon: FolderPlus,
    id: "local-directories",
    label: "本地目录",
  });

  return (
    <>
      <Tooltip
        content={
          selectedAgent
            ? `智能体：${selectedAgent.displayName || selectedAgent.name}`
            : selectedProject
              ? `项目：${selectedProject.name}`
              : (workspacePath ?? "选择工作空间")
        }
      >
        <button
          ref={triggerRef}
          type="button"
          className="flex min-w-0 max-w-full items-center gap-1 rounded px-2 py-1.5 text-xs text-foreground-secondary hover:bg-accent"
          onClick={(e) => {
            e.stopPropagation();
            toggle();
          }}
        >
          {selectedAgent ? (
            <Bot className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          ) : selectedProject ? (
            <FolderKanban className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          ) : (
            <FolderPlus className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          )}
          <span className="min-w-0 truncate">
            {selectedAgent
              ? (selectedAgent.displayName || selectedAgent.name)
              : (selectedProject?.name ?? workspaceName ?? "选择工作空间")}
          </span>
          <ChevronDown className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
        </button>
      </Tooltip>
      {open && (
        <DropdownMenu
          anchorElement={triggerRef.current}
          anchorRect={anchorRect}
          onClose={closeAll}
          placement="top"
          width={WS_MENU_WIDTH}
          items={menuItems}
        />
      )}
    </>
  );
}
