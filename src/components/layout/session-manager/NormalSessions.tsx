import { useState, type ReactNode } from "react";
import { ChevronRight, FileText, Loader2, MessageSquare, MoreHorizontal, Pencil, Pin, Trash2, Plus } from "lucide-react";
import { Tooltip } from "../../../components/ui";
import type { SessionGroup, SessionInfo } from "../../../types";
import { GroupRow, ItemRow } from "./SessionRows";
import { byUpdatedDesc, draftTitle, GroupDot } from "./sessionManagerShared";

// 会话子列表默认展示条数；超过后折叠，「展开显示」每次多显示这么多条。
const PAGE_SIZE = 5;

export function NormalSessions({
  busySessionId,
  collapsed,
  currentSessionId,
  emptyLabel,
  groups,
  onDeleteGroup,
  onEditGroup,
  onNewSession,
  onOpenDraft,
  onOpenSession,
  onOpenSessionMenu,
  onToggleCollapsed,
  sessions,
}: {
  busySessionId: string | null;
  collapsed: Record<string, boolean>;
  currentSessionId: string | null;
  emptyLabel?: string;
  groups: SessionGroup[];
  onDeleteGroup: (group: SessionGroup) => void;
  onEditGroup: (group: SessionGroup) => void;
  /** 会话区表头的悬浮「新会话」按钮（对齐项目区的 hover 操作）。 */
  onNewSession: () => void;
  onOpenDraft: (sessionId: string) => void;
  onOpenSession: (sessionId: string) => void;
  onOpenSessionMenu: (sessionId: string, x: number, y: number) => void;
  onToggleCollapsed: (key: string) => void;
  sessions: SessionInfo[];
}) {
  const visible = sessions
    .filter((s) => (!s.origin || s.origin === "user") && !s.projectId && !s.agentId)
    .sort(byUpdatedDesc);
  const pinned = visible.filter((s) => s.pinned);
  const pinnedIds = new Set(pinned.map((s) => s.id));
  const groupSections = groups.map((group) => ({
    group,
    members: visible
      .filter((s) => s.groupId === group.id && !pinnedIds.has(s.id))
      .sort(byUpdatedDesc),
  }));
  const recent = visible
    .filter((s) => !s.pinned && !s.groupId)
    .sort(byUpdatedDesc);

  // 每个会话子列表默认最多显示 PAGE_SIZE 条；点击「展开显示」每次多显示 PAGE_SIZE 条，
  // 点击「折叠显示」收回到 PAGE_SIZE 条。按分组 key 各自记住展开上限。
  const [limits, setLimits] = useState<Record<string, number>>({});
  const limitFor = (key: string) => limits[key] ?? PAGE_SIZE;
  const showMore = (key: string) =>
    setLimits((current) => ({ ...current, [key]: limitFor(key) + PAGE_SIZE }));
  const collapseList = (key: string) =>
    setLimits((current) => ({ ...current, [key]: PAGE_SIZE }));

  function renderPaged(key: string, list: SessionInfo[]) {
    const limit = limitFor(key);
    const hasMore = list.length > limit;
    const canCollapse = limit > PAGE_SIZE;
    return (
      <>
        {list.slice(0, limit).map(sessionRow)}
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

  function renderSessionActions(session: SessionInfo) {
    const busy = busySessionId === session.id;
    const active = session.id === currentSessionId;
    const actionTone = active
      ? "text-white/80 hover:bg-white/15 hover:text-white focus:opacity-100"
      : "text-foreground-muted hover:bg-accent hover:text-foreground focus:opacity-100";
    return (
      <button
        type="button"
        aria-label={`更多操作：${session.title}`}
        disabled={busy}
        onClick={(event) => {
          event.stopPropagation();
          onOpenSessionMenu(session.id, event.clientX - 148, event.clientY + 6);
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

  function sessionRow(session: SessionInfo) {
    const title = session.isDraft ? draftTitle(session) : session.title || "未命名会话";
    return (
      <ItemRow
        key={session.id}
        actions={renderSessionActions(session)}
        active={session.id === currentSessionId}
        label={session.isDraft ? (
        <span className="flex min-w-0 items-center gap-1.5">
          <FileText className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
          <span className="min-w-0 truncate">{title}</span>
        </span>
        ) : title}
        onClick={() => {
          if (session.isDraft) {
            onOpenDraft(session.id);
          } else {
            onOpenSession(session.id);
          }
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          onOpenSessionMenu(session.id, event.clientX, event.clientY);
        }}
        tooltip={session.isDraft ? session.draftContent || title : title}
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

  function groupActions(group: SessionGroup) {
    if (group.builtIn) return undefined;
    return (
      <div className="flex max-w-0 shrink-0 items-center gap-1 overflow-hidden opacity-0 transition-all duration-150 group-hover:ml-1 group-hover:max-w-[44px] group-hover:opacity-100 group-focus-within:ml-1 group-focus-within:max-w-[44px] group-focus-within:opacity-100">
        <Tooltip content="编辑分组">
          <button
            type="button"
            aria-label={`编辑分组：${group.label}`}
            onClick={(event) => {
              event.stopPropagation();
              onEditGroup(group);
            }}
            className="grid h-5 w-5 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
          >
            <Pencil className="h-3 w-3" aria-hidden="true" />
          </button>
        </Tooltip>
        <Tooltip content="删除分组">
          <button
            type="button"
            aria-label={`删除分组：${group.label}`}
            onClick={(event) => {
              event.stopPropagation();
              onDeleteGroup(group);
            }}
            className="grid h-5 w-5 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-destructive/10 hover:text-destructive"
          >
            <Trash2 className="h-3 w-3" aria-hidden="true" />
          </button>
        </Tooltip>
      </div>
    );
  }

  const hasContent = pinned.length > 0 || groupSections.some(({ members }) => members.length > 0) || recent.length > 0;
  const sectionExpanded = !collapsed["__sessions_section__"];

  return (
    <div className="flex flex-col gap-0.5">
      <div
        role="button"
        tabIndex={0}
        onClick={() => onToggleCollapsed("__sessions_section__")}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onToggleCollapsed("__sessions_section__");
          }
        }}
        className="group flex h-7 cursor-pointer items-center gap-1.5 px-2 text-foreground-muted"
      >
        <span className="min-w-0 truncate text-[13px] uppercase leading-none">
          会话
        </span>
        {/* <span className="shrink-0 text-xs font-normal">·</span>
        <span className="shrink-0 text-xs font-normal">{visible.length}</span> */}
        <ChevronRight
          className={`h-3.5 w-3.5 shrink-0 opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100 ${sectionExpanded ? "rotate-90" : ""}`}
          aria-hidden="true"
        />
        <span className="min-w-0 flex-1" />
        <span className="flex max-w-0 shrink-0 items-center gap-1 overflow-hidden opacity-0 transition-all duration-150 group-hover:ml-1 group-hover:max-w-[44px] group-hover:opacity-100 group-focus-within:ml-1 group-focus-within:max-w-[44px] group-focus-within:opacity-100">
          <Tooltip content="新会话">
            <button
              type="button"
              aria-label="新会话"
              onClick={(event) => {
                event.stopPropagation();
                onNewSession();
              }}
              className="grid h-5 w-5 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
            >
              <Plus className="h-3 w-3" aria-hidden="true" />
            </button>
          </Tooltip>
        </span>
      </div>
      {sectionExpanded && (
        <div className="flex flex-col gap-0.5">
          {hasContent ? (
            <>
              {pinned.length > 0 && (
                <GroupRow
                  badge={pinned.length}
                  expanded={!collapsed["__pinned__"]}
                  label="置顶"
                  onToggle={() => onToggleCollapsed("__pinned__")}
                >
                  {renderPaged("__pinned__", pinned)}
                </GroupRow>
              )}
              {groupSections.map(({ group, members }) =>
                members.length > 0 ? (
                  <GroupRow
                    key={group.id}
                    actions={groupActions(group)}
                    badge={members.length}
                    expanded={!collapsed[`group:${group.id}`]}
                    label={(
                      <span className="flex min-w-0 items-center gap-1.5">
                        <span className="min-w-0 truncate">{group.label}</span>
                        <GroupDot colorKey={group.colorKey} />
                      </span>
                    )}
                    onToggle={() => onToggleCollapsed(`group:${group.id}`)}
                  >
                    {renderPaged(`group:${group.id}`, members)}
                  </GroupRow>
                ) : null,
              )}
              {recent.length > 0 && (
                <GroupRow
                  badge={recent.length}
                  expanded={!collapsed["__recent__"]}
                  label="最近"
                  onToggle={() => onToggleCollapsed("__recent__")}
                >
                  {renderPaged("__recent__", recent)}
                </GroupRow>
              )}
            </>
          ) : (
            <EmptyRow label={emptyLabel ?? "暂无会话"} />
          )}
        </div>
      )}
    </div>
  );
}

function EmptyRow({ label }: { label: ReactNode }) {
  return (
    <div className="rounded-lg py-2 pr-2.5 text-[12px] text-foreground-muted" style={{ paddingLeft: 10 }}>
      {label}
    </div>
  );
}
