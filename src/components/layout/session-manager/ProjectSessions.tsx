import { useEffect, useState, type ReactNode } from "react";
import {
  ChevronRight,
  FolderKanban,
  FolderOpen,
  List,
  Loader2,
  MoreHorizontal,
  Pin,
  Plus,
} from "lucide-react";

import {
  listProjects,
  listProjectSessions,
} from "../../../api";
import { Tooltip } from "../../../components/ui";
import type { Project, SessionInfo } from "../../../types";
import { GroupRow, ItemRow } from "./SessionRows";
import { byUpdatedDesc } from "./sessionManagerShared";

// 项目会话列表默认展示条数；超过后折叠，「展开显示」每次多显示这么多条。
const PAGE_SIZE = 5;

type ProjectSessionGroup = {
  project: Project;
  sessions: SessionInfo[];
};

export function ProjectSessions({
  busySessionId,
  currentSessionId,
  onCreateProject,
  onOpenProject,
  onOpenProjectList,
  onOpenProjectSessionMenu,
  onOpenSession,
  onNewProjectSession,
  refreshKey,
}: {
  busySessionId: string | null;
  currentSessionId: string | null;
  onCreateProject: () => Promise<void> | void;
  onOpenProject: (projectId: string) => void;
  onOpenProjectList: () => void;
  onOpenProjectSessionMenu: (sessionId: string, x: number, y: number) => void;
  onOpenSession: (sessionId: string) => void;
  onNewProjectSession: (projectId: string) => void;
  refreshKey: string;
}) {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [sectionExpanded, setSectionExpanded] = useState(true);
  const [loading, setLoading] = useState(true);
  const [localRefreshKey, setLocalRefreshKey] = useState(0);
  const [projectGroups, setProjectGroups] = useState<ProjectSessionGroup[]>([]);
  // 每个项目的会话子列表默认最多显示 PAGE_SIZE 条；「展开显示」每次多显示 PAGE_SIZE 条，
  // 「折叠显示」收回到 PAGE_SIZE 条。按项目 key 各自记住展开上限。
  const [limits, setLimits] = useState<Record<string, number>>({});
  const limitFor = (key: string) => limits[key] ?? PAGE_SIZE;
  const showMore = (key: string) =>
    setLimits((current) => ({ ...current, [key]: limitFor(key) + PAGE_SIZE }));
  const collapseList = (key: string) =>
    setLimits((current) => ({ ...current, [key]: PAGE_SIZE }));

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      try {
        const projects = await listProjects();
        const groups = await Promise.all(
          projects.map(async (project) => ({
            project,
            sessions: (await listProjectSessions(project.id)).sort(byPinnedThenUpdated),
          })),
        );
        if (!cancelled) setProjectGroups(groups.sort((a, b) => b.project.updatedAt.localeCompare(a.project.updatedAt)));
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [localRefreshKey, refreshKey]);

  function toggleCollapsed(key: string) {
    setCollapsed((current) => ({ ...current, [key]: !current[key] }));
  }

  function createProjectSession(project: Project) {
    setLocalRefreshKey((key) => key + 1);
    onNewProjectSession(project.id);
  }

  function renderProjectSessionRow(session: SessionInfo) {
    return (
      <ItemRow
        key={session.id}
        actions={renderProjectSessionActions(session)}
        active={session.id === currentSessionId}
        label={session.title || "未命名会话"}
        onClick={() => onOpenSession(session.id)}
        onContextMenu={(event) => {
          event.preventDefault();
          onOpenProjectSessionMenu(session.id, event.clientX, event.clientY);
        }}
        tooltip={session.title || "未命名会话"}
        trailing={session.pinned ? (
          <span className="flex shrink-0 items-center gap-1">
            {session.isRunning && (
              <Loader2
                className="h-3.5 w-3.5 animate-spin text-foreground-muted"
                aria-hidden="true"
              />
            )}
            <Pin
              className="h-3.5 w-3.5 shrink-0 text-foreground-muted"
              aria-hidden="true"
            />
          </span>
        ) : session.isRunning ? (
          <Loader2
            className="h-3.5 w-3.5 shrink-0 animate-spin text-foreground-muted"
            aria-hidden="true"
          />
        ) : undefined}
      />
    );
  }

  function renderPagedSessions(key: string, list: SessionInfo[]) {
    const limit = limitFor(key);
    const hasMore = list.length > limit;
    const canCollapse = limit > PAGE_SIZE;
    return (
      <>
        {list.slice(0, limit).map(renderProjectSessionRow)}
        {list.length > PAGE_SIZE && (
          <div
            className="flex items-center gap-3 py-1 text-[12px] text-foreground-muted"
            style={{ paddingLeft: 25 }}
          >
            {hasMore && (
              <button
                type="button"
                onClick={() => showMore(key)}
                className="transition hover:text-foreground"
              >
                展开显示
              </button>
            )}
            {canCollapse && (
              <button
                type="button"
                onClick={() => collapseList(key)}
                className="transition hover:text-foreground"
              >
                折叠显示
              </button>
            )}
          </div>
        )}
      </>
    );
  }

  function renderProjectSessionActions(session: SessionInfo) {
    const busy = busySessionId === session.id;
    const active = session.id === currentSessionId;
    const actionTone = active
      ? "text-white/80 hover:bg-white/15 hover:text-white focus:opacity-100"
      : "text-foreground-muted hover:bg-accent hover:text-foreground focus:opacity-100";
    return (
      <button
        type="button"
        aria-label={`更多操作：${session.title || "未命名会话"}`}
        disabled={busy}
        onClick={(event) => {
          event.stopPropagation();
          onOpenProjectSessionMenu(session.id, event.clientX - 148, event.clientY + 6);
        }}
        className={`grid h-6 w-6 shrink-0 place-items-center rounded-md opacity-0 transition group-hover:opacity-100 disabled:opacity-60 ${actionTone}`}
      >
        {busy ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
        ) : (
          <MoreHorizontal className="h-3.5 w-3.5" aria-hidden="true" />
        )}
      </button>
    );
  }

  function renderRootActions() {
    return (
      <span className="flex max-w-0 shrink-0 items-center gap-1 overflow-hidden opacity-0 transition-all duration-150 group-hover:ml-1 group-hover:max-w-[44px] group-hover:opacity-100 group-focus-within:ml-1 group-focus-within:max-w-[44px] group-focus-within:opacity-100">
        <Tooltip content="新增项目">
          <button
            type="button"
            aria-label="新增项目"
            onClick={(event) => {
              event.stopPropagation();
              void onCreateProject();
            }}
            className="grid h-5 w-5 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
          >
            <Plus className="h-3 w-3" aria-hidden="true" />
          </button>
        </Tooltip>
        <Tooltip content="项目列表">
          <button
            type="button"
            aria-label="项目列表"
            onClick={(event) => {
              event.stopPropagation();
              onOpenProjectList();
            }}
            className="grid h-5 w-5 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
          >
            <List className="h-3 w-3" aria-hidden="true" />
          </button>
        </Tooltip>
      </span>
    );
  }

  function renderProjectActions(project: Project) {
    return (
      <span className="flex max-w-0 shrink-0 items-center gap-1 overflow-hidden opacity-0 transition-all duration-150 group-hover:ml-1 group-hover:max-w-[44px] group-hover:opacity-100 group-focus-within:ml-1 group-focus-within:max-w-[44px] group-focus-within:opacity-100">
        <Tooltip content="查看项目">
          <button
            type="button"
            aria-label={`查看项目：${project.name}`}
            onClick={(event) => {
              event.stopPropagation();
              onOpenProject(project.id);
            }}
            className="grid h-5 w-5 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
          >
            <FolderOpen className="h-3 w-3" aria-hidden="true" />
          </button>
        </Tooltip>
        <Tooltip content="新增项目会话">
          <button
            type="button"
            aria-label={`新增项目会话：${project.name}`}
            onClick={(event) => {
              event.stopPropagation();
              createProjectSession(project);
            }}
            className="grid h-5 w-5 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
          >
            <Plus className="h-3 w-3" aria-hidden="true" />
          </button>
        </Tooltip>
      </span>
    );
  }

  return (
    <div className="flex flex-col gap-0.5">
      <div
        role="button"
        tabIndex={0}
        onClick={() => setSectionExpanded((current) => !current)}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            setSectionExpanded((current) => !current);
          }
        }}
        className="group flex h-7 cursor-pointer items-center gap-1.5 px-2 text-foreground-muted"
      >
        <span className="min-w-0 truncate text-[13px] uppercase leading-none">
          项目
        </span>
        <ChevronRight
          className={`h-3.5 w-3.5 shrink-0 opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100 ${sectionExpanded ? "rotate-90" : ""}`}
          aria-hidden="true"
        />
        <span className="min-w-0 flex-1" />
        {renderRootActions()}
      </div>
      {sectionExpanded && (
        <div className="flex flex-col gap-0.5">
          {projectGroups.length > 0 ? (
            projectGroups.map(({ project, sessions }) => {
              const expanded = !collapsed[`project:${project.id}`];
              return (
                <GroupRow
                  key={project.id}
                  actions={renderProjectActions(project)}
                  badge={sessions.length}
                  expanded={expanded}
                  label={project.name}
                  onToggle={() => toggleCollapsed(`project:${project.id}`)}
                  tooltip={project.name}
                  icon={<FolderKanban className="h-3.5 w-3.5"  aria-hidden="true"/>}
                >
                  {sessions.length > 0 ? (
                    renderPagedSessions(`project:${project.id}`, sessions)
                  ) : (
                    <EmptyRow depth={1} label="暂无会话" />
                  )}
                </GroupRow>
              );
            })
          ) : (
            <EmptyRow label={loading ? "正在加载项目" : "暂无项目"} />
          )}
        </div>
      )}
    </div>
  );
}

function byPinnedThenUpdated(a: SessionInfo, b: SessionInfo) {
  const pinnedDiff = Number(Boolean(b.pinned)) - Number(Boolean(a.pinned));
  return pinnedDiff || byUpdatedDesc(a, b);
}

function EmptyRow({ depth = 0, label }: { depth?: number; label: ReactNode }) {
  return (
    <div
      className="rounded-lg py-2 pr-2.5 text-[12px] text-foreground-muted"
      style={{ paddingLeft: 10 + depth * 15 }}
    >
      {label}
    </div>
  );
}
