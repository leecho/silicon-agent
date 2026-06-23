import { AlertTriangle, ShieldAlert } from "lucide-react";
import { Button } from "../ui/Button";
import type { PendingPermission } from "../../types";
import { getToolNarrative } from "./toolNarrative";

// 风险工具执行前的权限确认卡：展示工具名 + 输入预览 + 允许/拒绝。
export function SessionPermissionCard({
  pending,
  busy,
  onDecide,
  agentLabel,
}: {
  pending: PendingPermission;
  busy: boolean;
  onDecide: (approved: boolean) => void;
  /** 子代理请求时传其名，卡片标注「子代理 X 需要确认」。 */
  agentLabel?: string;
}) {
  const toolDisplayName = getToolNarrative(pending.toolName);
  return (
    <div className="m-3 shrink-0 rounded-lg border border-border bg-card shadow-sm">
      <div className="flex items-center gap-3 px-4 py-3">
        <div className="grid h-6 w-6 shrink-0 place-items-center rounded-lg bg-muted text-danger">
          <ShieldAlert className="h-4 w-4" aria-hidden="true" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <h3 className="truncate text-sm font-semibold text-foreground">
              {agentLabel ? `子代理「${agentLabel}」需要确认权限` : "需要确认权限"}
            </h3>
            <span className="rounded-full bg-muted px-2 py-0.5 text-[11px] font-medium text-danger">
              高风险操作
            </span>
          </div>
          
        </div>
      </div>

      <div className="border-y flex flex-col gap-2 border-border-subtle bg-surface px-4 py-3">
        <p className="mt-1 text-[13px] leading-5 text-foreground-secondary">
            Agent 请求执行「{toolDisplayName}」。请确认输入内容符合预期后再允许。
        </p>
        <pre className="max-h-36 overflow-auto whitespace-pre-wrap rounded-lg border border-border-subtle bg-background px-3 py-2 font-mono text-[12px] leading-5 text-foreground-secondary [overflow-wrap:anywhere]">
          {pending.input || "无输入"}
          </pre>
      </div>

      <div className="flex items-center justify-end gap-2 px-4 py-3">
        <Button tone="danger" disabled={busy} onClick={() => onDecide(false)}>
          拒绝
        </Button>
        <Button tone="primary" disabled={busy} onClick={() => onDecide(true)}>
          允许执行
        </Button>
      </div>
    </div>
  );
}
