import { useEffect, useMemo, useState } from "react";
import { Bot, Check, Crown } from "lucide-react";
import { createTeam, listExperts } from "../../api";
import { avatarEmoji } from "../../lib/avatar";
import { Button } from "../../components/ui/Button";
import { Modal, ModalHeader } from "../../components/ui/Modal";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { ExpertSummary, Team, TeamMember } from "../../types";

/** 把 ExpertSummary 转成对它的引用（保留 owner 命名空间）。 */
function toMember(a: ExpertSummary, role: string): TeamMember {
  return { pluginId: a.pluginId, teamId: a.teamId, name: a.name, role };
}

/** 新建团队：填名称 + 从现有 agent 勾选成员、指定一个 lead。成员是**引用**（不复制）。 */
export function TeamBuilderModal({
  open,
  onClose,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  onCreated: (team: Team) => void;
}) {
  const notifications = useNotifications();
  const [agents, setExperts] = useState<ExpertSummary[]>([]);
  const [name, setName] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [description, setDescription] = useState("");
  // 选中成员的稳定键（pluginId|teamId|name）→ ExpertSummary；以及 lead 的键。
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [leadKey, setLeadKey] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const keyOf = (a: ExpertSummary) => `${a.pluginId}|${a.teamId}|${a.name}`;

  useEffect(() => {
    if (!open) return;
    setName("");
    setDisplayName("");
    setDescription("");
    setPicked(new Set());
    setLeadKey(null);
    listExperts()
      .then(setExperts)
      .catch((err) =>
        notifications.notify({ tone: "error", title: "加载专家失败", message: String(err) }),
      );
  }, [open, notifications]);

  const byKey = useMemo(() => {
    const m = new Map<string, ExpertSummary>();
    for (const a of agents) m.set(keyOf(a), a);
    return m;
  }, [agents]);

  function togglePick(a: ExpertSummary) {
    const k = keyOf(a);
    setPicked((prev) => {
      const next = new Set(prev);
      if (next.has(k)) {
        next.delete(k);
        if (leadKey === k) setLeadKey(null);
      } else {
        next.add(k);
      }
      return next;
    });
  }

  async function handleCreate() {
    if (!name.trim()) {
      notifications.notify({ tone: "error", title: "请填写团队标识", message: "用作团队的唯一标识，建议用英文" });
      return;
    }
    const lead = leadKey ? byKey.get(leadKey) : undefined;
    const members = Array.from(picked)
      .filter((k) => k !== leadKey)
      .map((k) => byKey.get(k))
      .filter((a): a is ExpertSummary => !!a)
      .map((a) => toMember(a, "member"));
    setSaving(true);
    try {
      const team = await createTeam(
        name.trim(),
        displayName.trim() || name.trim(),
        description.trim() || null,
        lead ? toMember(lead, "lead") : null,
        members,
      );
      onCreated(team);
    } catch (err) {
      notifications.notify({ tone: "error", title: "创建失败", message: String(err) });
    } finally {
      setSaving(false);
    }
  }

  const field =
    "w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-primary";

  return (
    <Modal open={open} onClose={onClose} title="新建团队">
      <ModalHeader onClose={onClose}>
        <h2 className="text-base font-semibold text-foreground">新建团队</h2>
        <p className="mt-0.5 text-xs text-foreground-muted">
          从现有的专家里挑几个当成员，再点皇冠指定一个主理人。主理人负责怎么安排、把活分给谁。
        </p>
      </ModalHeader>

      <div className="mt-4 space-y-3">
        <div className="flex gap-2">
          <input
            className={field}
            placeholder="团队标识（英文，唯一）"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <input
            className={field}
            placeholder="团队名称（如：交易台）"
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
          />
        </div>
        <textarea
          className={`${field} min-h-[56px] resize-y`}
          placeholder="团队描述（可选）"
          value={description}
          onChange={(e) => setDescription(e.target.value)}
        />

        <div>
          <p className="mb-1.5 text-xs font-medium text-foreground-muted">
            挑选成员（已选 {picked.size}）· 点皇冠指定为主理人
          </p>
          {agents.length === 0 ? (
            <div className="rounded-lg border border-dashed border-border px-4 py-6 text-center text-xs text-foreground-muted">
              还没有可选的专家。先去创建或导入一些专家，再回来组建团队。
            </div>
          ) : (
            <ul className="max-h-[280px] overflow-auto rounded-lg border border-border-subtle bg-surface">
              {agents.map((a) => {
                const k = keyOf(a);
                const isPicked = picked.has(k);
                const isLead = leadKey === k;
                return (
                  <li
                    key={k}
                    className="flex items-center gap-3 border-b border-border-subtle px-3 py-2 last:border-b-0"
                  >
                    <button
                      type="button"
                      onClick={() => togglePick(a)}
                      className={`grid h-6 w-6 shrink-0 place-items-center rounded border ${
                        isPicked
                          ? "border-primary bg-primary text-primary-foreground"
                          : "border-border text-transparent"
                      }`}
                      aria-label="选为团队成员"
                    >
                      <Check className="h-4 w-4" aria-hidden="true" />
                    </button>
                    <div className="grid h-7 w-7 shrink-0 place-items-center rounded-md border border-border bg-background text-[13px] text-foreground-secondary">
                      {avatarEmoji(a.avatar) ? (
                        <span aria-hidden="true">{avatarEmoji(a.avatar)}</span>
                      ) : (
                        <Bot className="h-4 w-4" />
                      )}
                    </div>
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-sm text-foreground">{a.displayName || a.name}</p>
                      {a.description && (
                        <p className="truncate text-xs text-foreground-muted">{a.description}</p>
                      )}
                    </div>
                    <button
                      type="button"
                      disabled={!isPicked}
                      onClick={() => setLeadKey(isLead ? null : k)}
                      title={isLead ? "取消主理人" : "设为主理人"}
                      className={`rounded-md p-1.5 transition ${
                        isLead
                          ? "text-amber-500"
                          : "text-foreground-muted hover:text-foreground disabled:opacity-30"
                      }`}
                    >
                      <Crown className="h-4 w-4" aria-hidden="true" />
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>

      <div className="mt-4 flex items-center justify-end gap-2">
        <Button tone="outline" onClick={onClose} disabled={saving}>
          取消
        </Button>
        <Button tone="primary" onClick={() => void handleCreate()} disabled={saving}>
          {saving ? "创建中…" : "创建"}
        </Button>
      </div>
    </Modal>
  );
}
