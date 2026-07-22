import { useState } from "react";
import { Bot, Plus, Trash2 } from "lucide-react";
import { deleteAgent } from "../../api";
import { avatarEmoji } from "../../lib/avatar";
import { Button } from "../../components/ui/Button";
import { useMessages } from "../../components/ui/MessageProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { AgentBuilderDrawer } from "./AgentBuilderDrawer";
import type { Agent } from "../../types";

/** 智能体列表：参考 ProjectList 的简单 grid 卡片；点卡片打开详情。 */
export function AgentList({
  agents,
  loading,
  onOpen,
  onCreated,
  onReload,
}: {
  agents: Agent[];
  loading: boolean;
  onOpen: (id: string) => void;
  onCreated: (agent: Agent) => void;
  onReload: () => void;
}) {
  const messages = useMessages();
  const notify = useNotifications();
  const [builderOpen, setBuilderOpen] = useState(false);

  async function handleDelete(a: Agent) {
    const ok = await messages.confirm({
      title: "删除智能体",
      message: `确定删除「${a.displayName || a.name}」吗？它的私有记忆会一并删除（历史会话保留）。操作不可撤销。`,
      tone: "warning",
      confirmText: "删除",
    });
    if (!ok) return;
    try {
      await deleteAgent(a.id);
      onReload();
    } catch (err) {
      notify.notify({ tone: "error", title: "删除失败", message: String(err) });
    }
  }

  return (
    <div className="h-full overflow-auto p-6 text-sm">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-6 mt-4 flex items-start justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold text-foreground">智能体</h1>
            <p className="mt-1 text-xs text-foreground-muted">
              由专家播种的常驻工作伙伴，保留独立人设、技能引用和私有记忆。
            </p>
          </div>
          <Button tone="primary" onClick={() => setBuilderOpen(true)}>
            <Plus className="h-4 w-4" aria-hidden="true" /> 新建智能体
          </Button>
        </div>

        {loading ? (
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {[0, 1, 2].map((i) => (
              <div key={i} className="h-28 animate-pulse rounded-xl border border-border-subtle bg-surface" />
            ))}
          </div>
        ) : agents.length === 0 ? (
          <EmptyState onCreate={() => setBuilderOpen(true)} />
        ) : (
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {agents.map((a) => (
              <AgentCard
                key={a.id}
                agent={a}
                onOpen={onOpen}
                onDelete={handleDelete}
              />
            ))}
          </div>
        )}
      </div>

      <AgentBuilderDrawer
        open={builderOpen}
        onClose={() => setBuilderOpen(false)}
        onCreated={(a) => {
          setBuilderOpen(false);
          onCreated(a);
        }}
      />
    </div>
  );
}

function AgentCard({
  agent,
  onOpen,
  onDelete,
}: {
  agent: Agent;
  onOpen: (id: string) => void;
  onDelete: (a: Agent) => void | Promise<void>;
}) {
  const emoji = avatarEmoji(agent.avatar);
  const title = agent.displayName || agent.name;
  const subtitle = agent.profession || agent.sourceExpertId;
  return (
    <button
      type="button"
      onClick={() => onOpen(agent.id)}
      className="group flex flex-col rounded-xl border border-border-subtle bg-surface p-4 text-left transition hover:border-border"
    >
      <div className="flex items-start gap-3">
        <span
          className={`grid h-9 w-9 shrink-0 place-items-center rounded-lg border border-border bg-background text-[18px] ${
            agent.enabled ? "text-primary" : "text-foreground-muted"
          }`}
        >
          {emoji ? <span aria-hidden="true">{emoji}</span> : <Bot className="h-4 w-4" aria-hidden="true" />}
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate font-semibold text-foreground">{title}</span>
          {subtitle && <span className="block truncate text-xs text-foreground-muted">{subtitle}</span>}
        </span>
        <span
          role="button"
          tabIndex={0}
          onClick={(e) => {
            e.stopPropagation();
            void onDelete(agent);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.stopPropagation();
              void onDelete(agent);
            }
          }}
          className="rounded-md px-1.5 py-1 text-foreground-muted opacity-0 transition hover:text-destructive group-hover:opacity-100"
        >
          <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
        </span>
      </div>

    </button>
  );
}

function EmptyState({ onCreate }: { onCreate: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-border bg-surface/40 py-16 text-foreground-muted">
      <div className="grid h-12 w-12 place-items-center rounded-full bg-muted">
        <Bot className="h-6 w-6" aria-hidden="true" />
      </div>
      <p className="text-sm">你还没有智能体</p>
      <Button tone="outline" onClick={onCreate}>
        <Plus className="h-4 w-4" aria-hidden="true" />
        新建智能体
      </Button>
    </div>
  );
}
