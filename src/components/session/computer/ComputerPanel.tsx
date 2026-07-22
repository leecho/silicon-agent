import { useCallback, useEffect, useRef, useState } from "react";
import { ShieldAlert, MonitorPlay } from "lucide-react";
import type { FeedRow } from "../../../types";
import {
  permissionOpenSettings,
  permissionRequest,
  permissionStatus,
} from "../../../api";
import { Button, EmptyState, Skeleton } from "../../ui";
import { useNotifications } from "../../ui/NotificationProvider";
import { computerCopy } from "./copy";
import { ComputerActionStream } from "./ComputerActionStream";

type PermPhase =
  | { kind: "loading" }
  | { kind: "granted" }
  | { kind: "blocked"; status: string } // denied / unknown
  | { kind: "error" };

export function ComputerPanel({
  rows,
  feedVersion,
  running,
  embedded,
}: {
  /** 当前会话 id（保留以兼容调用方；当前面板自身不直接使用）。 */
  sessionId?: string;
  /** 当前会话 feed 行（动作流内部按 computer 工具过滤）。 */
  rows: FeedRow[];
  /** feed 重渲染计数器：rows 数组原地 mutate，靠此值变化触发动作流重算。 */
  feedVersion: number;
  /** 会话是否运行中（驱动动作流的加载/停止态）。 */
  running: boolean;
  /** 嵌入 tab 壳：标题由 tab 承担，去掉自身标题头。 */
  embedded?: boolean;
}) {
  const notify = useNotifications();
  const [perm, setPerm] = useState<PermPhase>({ kind: "loading" });
  const aliveRef = useRef(true);

  const checkPermission = useCallback(() => {
    setPerm({ kind: "loading" });
    void permissionStatus("accessibility")
      .then((status) => {
        if (!aliveRef.current) return;
        if (status === "granted") setPerm({ kind: "granted" });
        else setPerm({ kind: "blocked", status });
      })
      .catch(() => {
        if (!aliveRef.current) return;
        setPerm({ kind: "error" });
      });
  }, []);

  useEffect(() => {
    aliveRef.current = true;
    checkPermission();
    return () => {
      aliveRef.current = false;
    };
  }, [checkPermission]);


  const openSettings = useCallback(() => {
    void permissionRequest("accessibility")
      .then((status) => {
        if (aliveRef.current && status === "granted") setPerm({ kind: "granted" });
      })
      .catch(() => {})
      .finally(() => {
        void permissionOpenSettings("accessibility").catch((err) => {
          notify.error({ title: computerCopy.permOpen, message: String(err) });
        });
      });
  }, [notify]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      {!embedded && (
        <div className="flex items-center gap-2 border-b border-border-subtle px-3 py-3">
          <MonitorPlay className="h-4 w-4 text-foreground-muted" aria-hidden="true" />
          <span className="text-sm font-semibold text-foreground">{computerCopy.featureName}</span>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto">
        {/* 加载态：检测权限中 → 骨架屏，布局稳定。 */}
        {perm.kind === "loading" && (
          <div className="flex flex-col gap-3 px-3 py-4">
            <Skeleton lines={3} />
          </div>
        )}

        {/* 错误态：权限检测本身失败 → 错误卡 + 重试（不留死路）。 */}
        {perm.kind === "error" && (
          <EmptyState
            variant="error"
            icon={<ShieldAlert className="h-6 w-6" aria-hidden="true" />}
            title={computerCopy.errorTitle}
            action={
              <Button tone="primary" onClick={checkPermission}>
                {computerCopy.retry}
              </Button>
            }
          />
        )}

        {/* 受阻态：未授予/未知 → 引导去系统设置开启 + 重新检测（两条出口）。 */}
        {perm.kind === "blocked" && (
          <EmptyState
            variant="error"
            icon={<ShieldAlert className="h-6 w-6" aria-hidden="true" />}
            title={computerCopy.permTitle}
            description={computerCopy.permBody}
            action={
              <div className="flex flex-wrap items-center justify-center gap-2">
                <Button tone="primary" onClick={openSettings}>
                  {computerCopy.permOpen}
                </Button>
                <Button tone="outline" onClick={checkPermission}>
                  {computerCopy.permRecheck}
                </Button>
              </div>
            }
          />
        )}

        {/* 已授予：桌面能力随总开关自动激活，无需手动「开始」。直接展示动作流（停止按钮仅运行中显示）。 */}
        {perm.kind === "granted" && (
          <div className="flex h-full min-h-0 flex-col">
            <div className="min-h-0 flex-1 overflow-y-auto">
              <ComputerActionStream
                rows={rows}
                feedVersion={feedVersion}
                running={running}
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
