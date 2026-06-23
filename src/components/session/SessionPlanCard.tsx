import { useState } from "react";
import { ClipboardList } from "lucide-react";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { MarkdownText } from "../ui/MarkdownText";
import type { PendingPlan } from "../../types";

// plan 模式下 propose_plan 暂停卡：展示标题/摘要/计划正文 + 批准执行 / 修改。
export function SessionPlanCard({
  plan,
  busy,
  onDecide,
}: {
  plan: PendingPlan;
  busy: boolean;
  onDecide: (approved: boolean, comment?: string) => void;
}) {
  const [comment, setComment] = useState("");
  const [editing, setEditing] = useState(false);
  return (
    <div className="m-3 shrink-0 rounded-2xl border border-border-subtle bg-card px-4 py-3 shadow-sm">
      <div className="mb-2 flex items-center gap-2 text-sm font-semibold text-foreground">
        <ClipboardList size={15} className="shrink-0 text-foreground-muted" />
        <span className="min-w-0 flex-1">{plan.title}</span>
        {plan.riskLevel && <Badge>{plan.riskLevel}</Badge>}
      </div>
      {plan.summary && (
        <div className="mb-2 text-[13px] text-foreground-secondary">
          {plan.summary}
        </div>
      )}
      <div className="mb-3 max-h-64 overflow-auto rounded-xl border border-border-subtle px-3 py-2 text-[13px] text-foreground-secondary">
        <MarkdownText
          value={plan.planMarkdown}
          className="max-w-full [overflow-wrap:anywhere]"
        />
      </div>
      {editing ? (
        <div className="flex flex-col gap-2">
          <textarea
            className="min-h-[64px] w-full rounded border border-border-subtle bg-card px-2 py-1 text-sm text-foreground placeholder:text-foreground-muted focus:outline-none focus:ring-1 focus:ring-ring"
            placeholder="说明需要修改的地方..."
            value={comment}
            disabled={busy}
            onChange={(e) => setComment(e.target.value)}
          />
          <div className="flex justify-end gap-2">
            <Button
              disabled={busy}
              onClick={() => {
                setEditing(false);
                setComment("");
              }}
            >
              取消
            </Button>
            <Button
              tone="primary"
              disabled={busy || comment.trim() === ""}
              onClick={() => onDecide(false, comment.trim())}
            >
              提交修改意见
            </Button>
          </div>
        </div>
      ) : (
        <div className="flex justify-end gap-2">
          <Button disabled={busy} onClick={() => setEditing(true)}>
            修改
          </Button>
          <Button tone="primary" disabled={busy} onClick={() => onDecide(true)}>
            批准执行
          </Button>
        </div>
      )}
    </div>
  );
}
