import { useEffect, useState } from "react";
import { listAgents } from "../../api";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { Agent } from "../../types";
import { AgentList } from "./AgentList";
import { AgentView } from "./AgentView";

/** 智能体页（参考 ProjectsPage 的 list↔view 结构）：列表加载 + 当前打开智能体的顶层切换。 */
export function AgentsPage({
  agentId,
  onBack,
  onNewScheduledTask,
  onOpenAgent,
  onOpenAgentList,
  onOpenScheduledTask,
}: {
  agentId?: string | null;
  onBack: () => void;
  onNewScheduledTask: (agentId: string) => void;
  onOpenAgent: (agentId: string) => void;
  onOpenAgentList: () => void;
  onOpenScheduledTask: (taskId: string) => void;
}) {
  const notify = useNotifications();
  const [agents, setAgents] = useState<Agent[]>([]);
  const [loading, setLoading] = useState(true);
  const [openId, setOpenId] = useState<string | null>(null);

  async function reload() {
    try {
      setAgents(await listAgents());
    } catch (err) {
      notify.notify({ tone: "error", title: "加载智能体失败", message: String(err) });
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void reload();
  }, []);

  useEffect(() => {
    if (agentId !== undefined) {
      setOpenId(agentId);
    }
  }, [agentId]);

  const open = agents.find((a) => a.id === openId) ?? null;
  if (open) {
    return (
      <AgentView
        agent={open}
        onBack={() => {
          onBack();
          void reload();
        }}
        onNewScheduledTask={onNewScheduledTask}
        onOpenScheduledTask={onOpenScheduledTask}
        onReload={() => void reload()}
      />
    );
  }

  return (
    <AgentList
      agents={agents}
      loading={loading}
      onOpen={onOpenAgent}
      onCreated={(a) => {
        void reload();
        onOpenAgent(a.id);
      }}
      onReload={() => {
        onOpenAgentList();
        void reload();
      }}
    />
  );
}
