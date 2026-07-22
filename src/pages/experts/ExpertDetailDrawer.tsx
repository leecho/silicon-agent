import { useEffect, useState } from "react";
import { Bot, Loader2, MessageSquarePlus, Sparkles } from "lucide-react";
import { getExpertDetail } from "../../api";
import { avatarEmoji } from "../../lib/avatar";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import { Drawer, DrawerHeader } from "../../components/ui/Drawer";
import { useSession } from "../../components/session/SessionProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { ExpertDetail } from "../../types";
import { SkillDetailDrawer } from "../skills/SkillDetailDrawer";
import { MarkdownText } from "../../components/ui";

/** 专家详情抽屉：身份 + 一句话描述 + 角色设定正文 + 「使用专家」入口。 */
export function ExpertDetailDrawer({
  expertId,
  onClose,
}: {
  expertId: string | null;
  onClose: () => void;
}) {
  const notifications = useNotifications();
  const { enterDraftWithExpert } = useSession();
  const [detail, setDetail] = useState<ExpertDetail | null>(null);
  const [selectedSkillId, setSelectedSkillId] = useState<string | null>(null);

  useEffect(() => {
    if (!expertId) {
      setDetail(null);
      setSelectedSkillId(null);
      return;
    }
    setSelectedSkillId(null);
    getExpertDetail(expertId)
      .then(setDetail)
      .catch((err) =>
        notifications.notify({ tone: "error", title: "加载详情失败", message: String(err) }),
      );
  }, [expertId, notifications]);

  const a = detail?.agent;
  const emoji = avatarEmoji(a?.avatar);

  function useExpert(prompt?: string) {
    if (!a) return;
    enterDraftWithExpert(a.id, prompt);
    closeDrawer();
  }

  function closeDrawer() {
    setSelectedSkillId(null);
    onClose();
  }

  return (
    <>
      <Drawer
        className="bg-popover text-popover-foreground"
        open={expertId !== null}
        onClose={closeDrawer}
        title={a?.displayName || a?.name}
        width="640px"
      >
      <DrawerHeader onClose={closeDrawer}>
        <div className="flex min-w-0 flex-1 items-center gap-3">
          <div className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-card text-[18px] text-foreground-secondary">
            {emoji ? <span aria-hidden="true">{emoji}</span> : <Bot className="h-5 w-5" aria-hidden="true" />}
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <h2 className="truncate text-base font-semibold text-foreground">
                {a?.displayName || a?.name || "专家详情"}
              </h2>
              {a?.profession && (
                <span className="shrink-0 text-xs text-foreground-muted">{a.profession}</span>
              )}
              {a?.source === "builtin" && <Badge tone="info">内置</Badge>}
              {a && !a.enabled && <Badge tone="neutral">已禁用</Badge>}
            </div>
          </div>
          {a && (
            <Button tone="primary" onClick={() => useExpert()}>
              <MessageSquarePlus className="h-4 w-4" aria-hidden="true" />
              使用专家
            </Button>
          )}
        </div>
      </DrawerHeader>

      <div className="min-h-0 overflow-auto bg-popover px-5 py-4">
        {!detail ? (
          <div className="grid h-full min-h-[200px] place-items-center text-sm text-foreground-muted">
            <div className="flex items-center gap-2">
              <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
              加载中...
            </div>
          </div>
        ) : (
          <>
            {detail.agent.description && (
              <p className="mb-5 text-sm leading-6 text-foreground-secondary [overflow-wrap:anywhere]">
                {detail.agent.description}
              </p>
            )}
            {detail.quickPrompts.length > 0 && (
              <div className="mb-5">
                <h3 className="mb-2 text-sm font-semibold text-foreground">试试这样问</h3>
                <ul className="space-y-1.5">
                  {detail.quickPrompts.map((q, i) => (
                    <li key={i}>
                      <button
                        type="button"
                        onClick={() => useExpert(q)}
                        className="w-full rounded-md border border-border-subtle bg-surface px-3 py-2 text-left text-[13px] text-foreground-secondary transition hover:border-primary hover:bg-accent"
                      >
                        {q}
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {detail.skills.length > 0 && (
              <div className="mb-5">
                <h3 className="mb-2 flex items-center gap-1.5 text-sm font-semibold text-foreground">
                  <Sparkles className="h-4 w-4 text-primary" aria-hidden="true" />
                  携带技能（{detail.skills.length}）
                </h3>
                <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
                  {detail.skills.map((s, i) => (
                    <li
                      key={s.id}
                      className={i === detail.skills.length - 1 ? "" : "border-b border-border-subtle"}
                    >
                      <button
                        type="button"
                        onClick={() => setSelectedSkillId(s.id)}
                        title="查看技能详情"
                        className="flex w-full items-start gap-2 px-4 py-2.5 text-left transition-colors hover:bg-accent"
                      >
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2">
                            <p className="truncate text-sm font-medium text-foreground">{s.name}</p>
                            {!s.enabled && <Badge tone="neutral">已禁用</Badge>}
                          </div>
                          {s.description && (
                            <p className="mt-0.5 line-clamp-2 text-xs leading-5 text-foreground-muted [overflow-wrap:anywhere]">
                              {s.description}
                            </p>
                          )}
                        </div>
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
            <h3 className="mb-2 text-sm font-semibold text-foreground">角色设定</h3>
                {detail.systemPrompt.trim() ? (
                  <MarkdownText value={detail.systemPrompt} className="rounded-lg border border-border-subtle bg-surface px-4 py-3 text-[13px] leading-6 text-foreground-secondary [overflow-wrap:anywhere]" />
             
            ) : (
              <div className="rounded-lg border border-dashed border-border px-4 py-6 text-center text-sm text-foreground-muted">
                这个专家还没有角色设定
              </div>
            )}
            <p className="mt-4 text-xs text-foreground-muted">
              点「使用专家」开一段新对话——助手会以它的身份和设定来帮你。
            </p>
          </>
        )}
      </div>
      </Drawer>
      <SkillDetailDrawer skillId={selectedSkillId} onClose={() => setSelectedSkillId(null)} />
    </>
  );
}
