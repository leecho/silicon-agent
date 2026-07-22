import { useState } from "react";
import { Bot } from "lucide-react";

import { Tooltip } from "../../components/ui/Tooltip";
import type { ExpertSummary } from "../../types";

function agentLabel(agent: ExpertSummary) {
  return agent.displayName?.trim() || agent.name;
}

function agentTooltip(agent: ExpertSummary) {
  return agent.description?.trim() || agentLabel(agent);
}

function orderedPopularExperts(agents: ExpertSummary[], seedRole?: { kind: string; id: string } | null) {
  const enabledExperts = agents.filter((agent) => agent.enabled);
  if (seedRole?.kind !== "expert") return enabledExperts.slice(0, 6);
  return [...enabledExperts]
    .sort((left, right) => Number(right.id === seedRole.id) - Number(left.id === seedRole.id))
    .slice(0, 6);
}

export function PopularExpertsBar({
  agents,
  onPickExpert,
  roleId,
  roleKind,
  seedRole,
}: {
  agents: ExpertSummary[];
  onPickExpert: (expertId: string) => void;
  roleId?: string | null;
  roleKind?: string | null;
  seedRole?: { kind: string; id: string } | null;
}) {
  const [failedAvatarIds, setFailedAvatarIds] = useState<Set<string>>(() => new Set());
  const popularExperts = orderedPopularExperts(agents, seedRole);
  if (popularExperts.length === 0) return null;

  return (
    <section className="pl-2 mt-4 min-w-0" aria-label="常用专家">
      <div className="flex min-w-0 justify-center gap-1.5 overflow-x-auto px-1 pb-1">
        {popularExperts.map((agent) => {
          const selected =
            (roleKind === "expert" && roleId === agent.id) ||
            (seedRole?.kind === "expert" && seedRole.id === agent.id && !roleId);
          const avatarSrc = agent.avatar ?? undefined;
          const showAvatar = !!avatarSrc && !failedAvatarIds.has(agent.id);
          return (
            <Tooltip key={agent.id} content={agentTooltip(agent)}>
              <button
                type="button"
                onClick={() => onPickExpert(agent.id)}
                className={`flex h-9 min-w-[96px] max-w-[140px] shrink-0 items-center gap-1.5 rounded-lg border px-2 text-left transition ${
                  selected
                    ? "border-primary bg-primary/10 text-foreground"
                    : "border-border-subtle bg-surface text-foreground-secondary hover:border-border hover:bg-accent"
                }`}
                aria-pressed={selected}
              >
                {showAvatar ? (
                  <img
                    alt=""
                    className="h-5 w-5 shrink-0 rounded-full object-cover"
                    onError={() => setFailedAvatarIds((current) => new Set(current).add(agent.id))}
                    src={avatarSrc}
                  />
                ) : (
                  <span className="grid h-5 w-5 shrink-0 place-items-center rounded-full bg-background text-foreground-muted">
                    <Bot className="h-3.5 w-3.5" aria-hidden="true" />
                  </span>
                )}
                <span className="min-w-0 flex-1 truncate text-[12px] font-medium">
                  {agentLabel(agent)}
                </span>
              </button>
            </Tooltip>
          );
        })}
      </div>
    </section>
  );
}
