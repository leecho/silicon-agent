import { useEffect, useState } from "react";
import { Bot, Crown, Loader2, Sparkles, Users } from "lucide-react";
import { getTeamDetail } from "../../api";
import { avatarEmoji } from "../../lib/avatar";
import { Badge } from "../../components/ui/Badge";
import { Drawer, DrawerHeader } from "../../components/ui/Drawer";
import { useSession } from "../../components/session/SessionProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { TeamDetail } from "../../types";
import { SkillDetailDrawer } from "../skills/SkillDetailDrawer";
import { ExpertDetailDrawer } from "../experts/ExpertDetailDrawer";

/** 团队详情抽屉：lead（作 SOP）+ 成员（roster）+ 开场引导语。 */
export function TeamDetailDrawer({
  teamId,
  onClose,
}: {
  teamId: string | null;
  onClose: () => void;
}) {
  const notifications = useNotifications();
  const { enterDraftWithTeam } = useSession();
  const [detail, setDetail] = useState<TeamDetail | null>(null);
  const [selectedSkillId, setSelectedSkillId] = useState<string | null>(null);
  const [selectedExpertId, setSelectedExpertId] = useState<string | null>(null);

  // 点开场引导语 = 激活该团队 + 预填该提示词，开一段新对话。
  function useTeam(prompt?: string) {
    if (!teamId) return;
    enterDraftWithTeam(teamId, prompt);
    closeDrawer();
  }

  function closeDrawer() {
    setSelectedSkillId(null);
    setSelectedExpertId(null);
    onClose();
  }

  useEffect(() => {
    if (!teamId) {
      setDetail(null);
      setSelectedSkillId(null);
      setSelectedExpertId(null);
      return;
    }
    setSelectedSkillId(null);
    setSelectedExpertId(null);
    getTeamDetail(teamId)
      .then(setDetail)
      .catch((err) =>
        notifications.notify({ tone: "error", title: "加载详情失败", message: String(err) }),
      );
  }, [teamId, notifications]);

  const team = detail?.team;

  return (
    <>
      <Drawer
        className="bg-popover text-popover-foreground"
        open={teamId !== null}
        onClose={closeDrawer}
        title={team?.displayName}
        width="min(640px, 94vw)"
      >
        <DrawerHeader onClose={closeDrawer}>
        <div className="flex min-w-0 items-center gap-3">
          <div className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-card text-foreground-secondary">
            <Users className="h-5 w-5" aria-hidden="true" />
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <h2 className="truncate text-base font-semibold text-foreground">
                {team?.displayName ?? "团队详情"}
              </h2>
              {team && (
                <>
                  <Badge tone={team.enabled ? "success" : "neutral"}>
                    {team.enabled ? "已启用" : "已禁用"}
                  </Badge>
                  <Badge tone="neutral">{team.memberCount} 成员</Badge>
                </>
              )}
            </div>
          </div>
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
            {detail.team.description && (
              <p className="mb-5 whitespace-pre-wrap text-sm leading-6 text-foreground-secondary [overflow-wrap:anywhere]">
                {detail.team.description}
              </p>
            )}

            {detail.lead && (
              <div className="mb-5">
                <h3 className="mb-2 flex items-center gap-1.5 text-sm font-semibold text-foreground">
                  <Crown className="h-4 w-4 text-amber-500" aria-hidden="true" />
                  主理人（负责怎么安排、把活分给谁）
                </h3>
                <div className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
                  <MemberRow
                    avatar={detail.lead.avatar}
                    title={detail.lead.displayName || detail.lead.name}
                    profession={detail.lead.profession}
                    description={detail.lead.description}
                    onClick={() => setSelectedExpertId(detail.lead!.id)}
                  />
                </div>
              </div>
            )}

            <h3 className="mb-2 text-sm font-semibold text-foreground">
              成员（{detail.members.length}）
            </h3>
            {detail.members.length === 0 ? (
              <div className="rounded-lg border border-dashed border-border px-4 py-6 text-center text-sm text-foreground-muted">
                这个团队还没有成员
              </div>
            ) : (
              <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
                {detail.members.map((m, i) => (
                  <li
                    key={`${m.name}-${i}`}
                    className={i === detail.members.length - 1 ? "" : "border-b border-border-subtle"}
                  >
                    <MemberRow
                      avatar={m.avatar}
                      title={m.displayName || m.name}
                      profession={m.profession}
                      description={m.description}
                      onClick={() => setSelectedExpertId(m.id)}
                    />
                  </li>
                ))}
              </ul>
            )}

            {detail.skills.length > 0 && (
              <div className="mt-5">
                <h3 className="mb-2 flex items-center gap-1.5 text-sm font-semibold text-foreground">
                  <Sparkles className="h-4 w-4 text-primary" aria-hidden="true" />
                  团队技能（{detail.skills.length}）
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
                        className="block w-full px-4 py-2.5 text-left transition-colors hover:bg-accent"
                      >
                        <div className="flex items-center gap-2">
                          <p className="truncate text-sm font-medium text-foreground">{s.name}</p>
                          {!s.enabled && <Badge tone="neutral">已禁用</Badge>}
                        </div>
                        {s.description && (
                          <p className="mt-0.5 line-clamp-2 text-xs leading-5 text-foreground-muted [overflow-wrap:anywhere]">
                            {s.description}
                          </p>
                        )}
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {detail.quickPrompts.length > 0 && (
              <div className="mt-5">
                <h3 className="mb-2 text-sm font-semibold text-foreground">开场引导语</h3>
                <ul className="space-y-1.5">
                  {detail.quickPrompts.map((q, i) => (
                    <li key={i}>
                      <button
                        type="button"
                        onClick={() => useTeam(q)}
                        className="w-full rounded-md border border-border-subtle bg-surface px-3 py-2 text-left text-[13px] text-foreground-secondary transition hover:border-primary hover:bg-accent"
                      >
                        {q}
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
            <p className="mt-4 text-xs text-foreground-muted">
              在输入框的选择器（👥）里激活这个团队后，主助手会按主理人的安排，把任务交给上面的成员完成。
            </p>
          </>
        )}
      </div>
      </Drawer>
      <SkillDetailDrawer skillId={selectedSkillId} onClose={() => setSelectedSkillId(null)} />
      <ExpertDetailDrawer expertId={selectedExpertId} onClose={() => setSelectedExpertId(null)} />
    </>
  );
}

function MemberRow({
  avatar,
  title,
  profession,
  description,
  onClick,
}: {
  avatar?: string | null;
  title: string;
  profession?: string | null;
  description?: string;
  onClick?: () => void;
}) {
  const body = (
    <div className="flex items-center gap-3">
      <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-border bg-background text-[15px] text-foreground-secondary">
        {avatarEmoji(avatar) ? (
          <span aria-hidden="true">{avatarEmoji(avatar)}</span>
        ) : (
          <Bot className="h-4 w-4" aria-hidden="true" />
        )}
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <p className="truncate text-sm font-medium text-foreground">{title}</p>
          {profession && (
            <span className="shrink-0 text-xs text-foreground-muted">{profession}</span>
          )}
        </div>
        {description && (
          <p className="mt-0.5 line-clamp-2 text-xs leading-5 text-foreground-muted [overflow-wrap:anywhere]">
            {description}
          </p>
        )}
      </div>
    </div>
  );

  if (!onClick) {
    return <div className="px-4 py-3">{body}</div>;
  }
  return (
    <button
      type="button"
      onClick={onClick}
      title="查看成员详情"
      className="block w-full px-4 py-3 text-left transition-colors hover:bg-accent"
    >
      {body}
    </button>
  );
}
