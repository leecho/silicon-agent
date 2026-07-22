import { Bot, FileText, FolderOpen, Pencil } from "lucide-react";
import { MarkdownText } from "../../components/ui/MarkdownText";
import { Tooltip } from "../../components/ui/Tooltip";
import { avatarEmoji } from "../../lib/avatar";
import type { Agent } from "../../types";

function baseName(p: string): string {
  const t = p.replace(/[/\\]+$/, "");
  const i = Math.max(t.lastIndexOf("/"), t.lastIndexOf("\\"));
  return i >= 0 ? t.slice(i + 1) : t;
}

/** 常驻概况区：头像 / 名称 / 职业 / 工作目录 + 编辑身份。始终显示在智能体详情顶部，不随 Tab 切换。 */
export function AgentOverviewPanel({
  agent,
  onOpenWorkspace,
  onEditIdentity,
}: {
  agent: Agent;
  onOpenWorkspace: () => void;
  onEditIdentity: () => void;
}) {
  const emoji = avatarEmoji(agent.avatar);
  const title = agent.displayName || agent.name;
  const wsName = agent.workingDir ? baseName(agent.workingDir) : undefined;
  const wsLabel = wsName ?? "默认工作目录";
  return (
    <div>
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <span className={`grid h-12 w-12 shrink-0 place-items-center rounded-xl border border-border-subtle bg-background text-[22px] ${agent.enabled ? "text-primary" : "text-foreground-muted"}`}>
            {emoji ? <span aria-hidden="true">{emoji}</span> : <Bot className="h-6 w-6" aria-hidden="true" />}
          </span>
          <div className="min-w-0 pt-0.5">
            <h2 className="truncate text-base font-semibold text-foreground">{title}</h2>
            {agent.profession?.trim() && <p className="mt-1 line-clamp-2 text-[13px] leading-6 text-foreground-secondary">{agent.profession}</p>}
            
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-3">

          <Tooltip content="点击打开工作目录">
                <button type="button" onClick={onOpenWorkspace} className="flex items-center gap-1 text-[12px] text-primary">
                  <FolderOpen className="h-3.5 w-3.5" aria-hidden="true" /> 工作目录
                </button>
              </Tooltip>
          <button type="button" onClick={onEditIdentity} className="flex shrink-0 items-center gap-1 text-[12px] text-primary">
          <Pencil className="h-3.5 w-3.5" aria-hidden="true" /> 编辑
        </button>
        </div>
        
      </div>
    </div>
  );
}

/** 身份人格 Tab：身份锚 / 人格 预览 + 编辑 + 自我演化开关 + 演化提案入口。 */
export function AgentIdentityPanel({
  agent,
  pendingSoulProposalCount,
  onSetEvolutionEnabled,
  onEditIdentityAnchor,
  onViewIdentityAnchor,
  onEditSoul,
  onViewInstructions,
  onOpenEvolution,
}: {
  agent: Agent;
  pendingSoulProposalCount: number;
  onSetEvolutionEnabled: (enabled: boolean) => void | Promise<void>;
  onEditIdentityAnchor: () => void;
  onViewIdentityAnchor: () => void;
  onEditSoul: () => void;
  onViewInstructions: () => void;
  onOpenEvolution: () => void;
}) {
  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto max-w-[860px]">
        <section className="rounded-xl border border-border-subtle bg-surface p-4">
          <div className="mb-3 flex items-center justify-between gap-3">
            <h3 className="flex items-center gap-1.5 text-sm font-semibold text-foreground">
              <FileText className="h-4 w-4 text-foreground-secondary" aria-hidden="true" />
              身份与人格
            </h3>
            <div className="flex min-w-0 items-center gap-2">
              <Tooltip content="开启后会在攒够新经历时提出人格更新，提案需批准后生效">
                <button
                  type="button"
                  onClick={() => void onSetEvolutionEnabled(!agent.evolutionEnabled)}
                  className={`inline-flex h-5 w-9 shrink-0 items-center rounded-full transition ${agent.evolutionEnabled ? "bg-primary" : "bg-border"}`}
                  aria-label={agent.evolutionEnabled ? "关闭自我演化" : "开启自我演化"}
                  title={agent.evolutionEnabled ? "关闭自我演化" : "开启自我演化"}
                >
                  <span className={`inline-block h-4 w-4 rounded-full bg-white transition ${agent.evolutionEnabled ? "translate-x-4" : "translate-x-0.5"}`} />
                </button>
              </Tooltip>
              <span className="text-[12px] font-medium text-foreground-secondary">自我演化</span>
            </div>
          </div>
          <div className="space-y-2">
            <div className="rounded-lg border border-border-subtle bg-background px-3 py-2.5">
              <div className="mb-1 flex items-center justify-between gap-2">
                <div className="text-[13px] font-semibold text-foreground-secondary">身份</div>
                <button type="button" onClick={onEditIdentityAnchor} className="flex items-center gap-1 text-[12px] text-primary hover:text-foreground">
                  <Pencil className="h-3.5 w-3.5" aria-hidden="true" />
                  编辑
                </button>
              </div>
              {agent.identity?.trim() ? (
                <div className="cursor-pointer" onClick={onViewIdentityAnchor}>
                  <MarkdownText value={agent.identity} className="text-[12px] leading-5 text-foreground-secondary" />
                </div>
              ) : (
                <p className="cursor-pointer text-[12px] leading-5 text-foreground-secondary" onClick={onViewIdentityAnchor}>
                  未设置身份锚。
                </p>
              )}
            </div>
            <div className="rounded-lg border border-border-subtle bg-background px-3 py-2.5">
              <div className="mb-1 flex items-center justify-between gap-2">
                <div className="text-[13px] font-semibold text-foreground-secondary">人格</div>
                <button type="button" onClick={onEditSoul} className="flex items-center gap-1 text-[12px] text-primary hover:text-foreground">
                  <Pencil className="h-3.5 w-3.5" aria-hidden="true" />
                  编辑
                </button>
              </div>
              {agent.instructions?.trim() ? (
                <div className="cursor-pointer" onClick={onViewInstructions}>
                  <MarkdownText value={agent.instructions} className="text-[12px] leading-5 text-foreground-secondary" />
                </div>
              ) : (
                <p className="cursor-pointer text-[12px] leading-5 text-foreground-secondary" onClick={onViewInstructions}>
                  未设置人格。
                </p>
              )}
              <div className="mt-2 flex items-center justify-between gap-3 border-t border-border-subtle py-2">
                <button type="button" onClick={onOpenEvolution} className="shrink-0 text-[12px] text-primary hover:text-foreground">
                  {pendingSoulProposalCount > 0 ? `${pendingSoulProposalCount} 个演化提案` : "暂无演化提案"}
                </button>
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
