import { useEffect, useState, type ReactNode } from "react";
import {
  Bot,
  ChevronRight,
  Eye,
  List,
  Loader2,
  MoreHorizontal,
  Pin,
  Plus,
} from "lucide-react";

import { listAgents, listAgentSessions } from "../../../api";
import { Tooltip } from "../../../components/ui";
import type { Agent, SessionInfo } from "../../../types";
import { GroupRow, ItemRow } from "./SessionRows";
import { byUpdatedDesc } from "./sessionManagerShared";

type AgentSessionGroup = {
  agent: Agent;
  sessions: SessionInfo[];
};

export function AgentSessions({
  busySessionId,
  currentSessionId,
  onCreateAgent,
  onOpenAgent,
  onOpenAgentList,
  onOpenAgentSessionMenu,
  onOpenSession,
  onNewAgentSession,
  refreshKey,
}: {
  busySessionId: string | null;
  currentSessionId: string | null;
  onCreateAgent: () => Promise<void> | void;
  onOpenAgent: (agentId: string) => void;
  onOpenAgentList: () => void;
  onOpenAgentSessionMenu: (sessionId: string, x: number, y: number) => void;
  onOpenSession: (sessionId: string) => void;
  onNewAgentSession: (agentId: string) => void;
  refreshKey: string;
}) {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [sectionExpanded, setSectionExpanded] = useState(true);
  const [loading, setLoading] = useState(true);
  const [localRefreshKey, setLocalRefreshKey] = useState(0);
  const [agentGroups, setAgentGroups] = useState<AgentSessionGroup[]>([]);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setLoading(true);
      try {
        const agents = await listAgents();
        const groups = await Promise.all(
          agents.map(async (agent) => ({
            agent,
            sessions: (await listAgentSessions(agent.id)).sort(byPinnedThenUpdated),
          })),
        );
        if (!cancelled) setAgentGroups(groups.sort((a, b) => b.agent.updatedAt.localeCompare(a.agent.updatedAt)));
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

  function createAgentSession(agent: Agent) {
    setLocalRefreshKey((key) => key + 1);
    onNewAgentSession(agent.id);
  }

  function renderAgentSessionActions(session: SessionInfo) {
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
          onOpenAgentSessionMenu(session.id, event.clientX - 148, event.clientY + 6);
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
        <Tooltip content="新增智能体">
          <button
            type="button"
            aria-label="新增智能体"
            onClick={(event) => {
              event.stopPropagation();
              void onCreateAgent();
            }}
            className="grid h-5 w-5 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
          >
            <Plus className="h-3 w-3" aria-hidden="true" />
          </button>
        </Tooltip>
        <Tooltip content="智能体列表">
          <button
            type="button"
            aria-label="智能体列表"
            onClick={(event) => {
              event.stopPropagation();
              onOpenAgentList();
            }}
            className="grid h-5 w-5 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
          >
            <List className="h-3 w-3" aria-hidden="true" />
          </button>
        </Tooltip>
      </span>
    );
  }

  function renderAgentActions(agent: Agent) {
    return (
      <span className="flex max-w-0 shrink-0 items-center gap-1 overflow-hidden opacity-0 transition-all duration-150 group-hover:ml-1 group-hover:max-w-[44px] group-hover:opacity-100 group-focus-within:ml-1 group-focus-within:max-w-[44px] group-focus-within:opacity-100">
        <Tooltip content="查看智能体">
          <button
            type="button"
            aria-label={`查看智能体：${agent.displayName ?? agent.name}`}
            onClick={(event) => {
              event.stopPropagation();
              onOpenAgent(agent.id);
            }}
            className="grid h-5 w-5 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
          >
            <Eye className="h-3 w-3" aria-hidden="true" />
          </button>
        </Tooltip>
        <Tooltip content="新增智能体会话">
          <button
            type="button"
            aria-label={`新增智能体会话：${agent.displayName ?? agent.name}`}
            onClick={(event) => {
              event.stopPropagation();
              createAgentSession(agent);
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
          智能体
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
          {agentGroups.length > 0 ? (
            agentGroups.map(({ agent, sessions }) => {
              const agentLabel = agent.displayName ?? agent.name;
              const expanded = !collapsed[`agent:${agent.id}`];
              return (
                <GroupRow
                  key={agent.id}
                  actions={renderAgentActions(agent)}
                  badge={sessions.length}
                  expanded={expanded}
                  icon={<Bot className="h-3.5 w-3.5" aria-hidden="true" />}
                  label={agentLabel}
                  onToggle={() => toggleCollapsed(`agent:${agent.id}`)}
                  tooltip={agentLabel}
                >
                  {sessions.length > 0 ? (
                    sessions.map((session) => (
                      <ItemRow
                        key={session.id}
                        actions={renderAgentSessionActions(session)}
                        active={session.id === currentSessionId}
                        label={session.title || "未命名会话"}
                        onClick={() => onOpenSession(session.id)}
                        onContextMenu={(event) => {
                          event.preventDefault();
                          onOpenAgentSessionMenu(session.id, event.clientX, event.clientY);
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
                    ))
                  ) : (
                    <EmptyRow depth={1} label="暂无会话" />
                  )}
                </GroupRow>
              );
            })
          ) : (
            <EmptyRow label={loading ? "正在加载智能体" : "暂无智能体"} />
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
