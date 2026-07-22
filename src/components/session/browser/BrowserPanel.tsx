import { useCallback, useEffect, useRef, useState } from "react";
import { Globe, MonitorX } from "lucide-react";
import type { FeedRow } from "../../../types";
import { browserIsOpen, browserOpen, browserStatus, getBrowserHeadless } from "../../../api";
import { Button, EmptyState, Skeleton } from "../../ui";
import { useNotifications } from "../../ui/NotificationProvider";
import { browserCopy } from "./copy";
import { BrowserActionStream } from "./BrowserActionStream";

type StatusPhase =
  | { kind: "loading" }
  | { kind: "ready" }
  | { kind: "not_installed" }
  | { kind: "error" };

export function BrowserPanel({
  rows,
  feedVersion,
  running,
  embedded,
}: {
  /** 当前会话 id（保留以兼容调用方；当前面板自身不直接使用）。 */
  sessionId?: string;
  /** 当前会话 feed 行（动作流内部按 browser 工具过滤）。 */
  rows: FeedRow[];
  /** feed 重渲染计数器：rows 数组原地 mutate，靠此值变化触发动作流重算。 */
  feedVersion: number;
  /** 会话是否运行中（驱动动作流的加载/停止态）。 */
  running: boolean;
  /** 嵌入 tab 壳：标题由 tab 承担，去掉自身标题头。 */
  embedded?: boolean;
}) {
  const notify = useNotifications();
  const [phase, setPhase] = useState<StatusPhase>({ kind: "loading" });
  const [headless, setHeadless] = useState(false);
  // 常驻浏览器当前是否开着：决定是否显示「关闭浏览器」+ 是否还需提示登录（来自后端 browser_is_open）。
  const [isOpen, setIsOpen] = useState(false);
  const [opening, setOpening] = useState(false);
  const aliveRef = useRef(true);

  const checkStatus = useCallback(() => {
    setPhase({ kind: "loading" });
    void browserStatus()
      .then((status) => {
        if (!aliveRef.current) return;
        if (status === "ready") setPhase({ kind: "ready" });
        else setPhase({ kind: "not_installed" });
      })
      .catch(() => {
        if (!aliveRef.current) return;
        setPhase({ kind: "error" });
      });
  }, []);

  const refreshIsOpen = useCallback(() => {
    void browserIsOpen()
      .then((v) => { if (aliveRef.current) setIsOpen(v); })
      .catch(() => {});
  }, []);

  useEffect(() => {
    aliveRef.current = true;
    checkStatus();
    getBrowserHeadless().then((v) => { if (aliveRef.current) setHeadless(v); }).catch(() => {});
    return () => {
      aliveRef.current = false;
    };
  }, [checkStatus]);

  // 「浏览器开没开」需跟随真实状态：进面板/有浏览器活动时即时拉，外加轮询兜住「空闲超时自动关」
  //（后端自动关窗没有前端事件，只能靠轮询发现）。
  useEffect(() => {
    refreshIsOpen();
    const t = setInterval(refreshIsOpen, 5000);
    return () => clearInterval(t);
  }, [refreshIsOpen]);
  useEffect(() => {
    refreshIsOpen();
  }, [feedVersion, running, refreshIsOpen]);

  const doOpen = useCallback(() => {
    setOpening(true);
    void browserOpen()
      .then(() => {
        if (!aliveRef.current) return;
        setOpening(false);
        setIsOpen(true); // 乐观：窗口已开 → 收起登录卡、露出「关闭浏览器」
        refreshIsOpen();
      })
      .catch((err) => {
        if (!aliveRef.current) return;
        setOpening(false);
        notify.error({ title: browserCopy.errorTitle, message: String(err) });
      });
  }, [notify, refreshIsOpen]);


  return (
    <div className="flex h-full min-h-0 flex-col">
      {!embedded && (
        <div className="flex items-center gap-2 border-b border-border-subtle px-3 py-3">
          <Globe className="h-4 w-4 text-foreground-muted" aria-hidden="true" />
          <span className="text-sm font-semibold text-foreground">{browserCopy.featureName}</span>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto">
        {/* 加载态：检测浏览器中 → 骨架屏，布局稳定。 */}
        {phase.kind === "loading" && (
          <div className="flex flex-col gap-3 px-3 py-4">
            <Skeleton lines={3} />
          </div>
        )}

        {/* 错误态：检测本身失败 → 错误卡 + 重试（不留死路）。 */}
        {phase.kind === "error" && (
          <EmptyState
            variant="error"
            icon={<MonitorX className="h-6 w-6" aria-hidden="true" />}
            title={browserCopy.errorTitle}
            action={
              <Button tone="primary" onClick={checkStatus}>
                {browserCopy.retry}
              </Button>
            }
          />
        )}

        {/* 未安装 Chrome：引导安装 + 重新检测（两条出口）。 */}
        {phase.kind === "not_installed" && (
          <EmptyState
            variant="error"
            icon={<MonitorX className="h-6 w-6" aria-hidden="true" />}
            title={browserCopy.noChromeTitle}
            description={browserCopy.noChromeBody}
            action={
              <Button tone="primary" onClick={checkStatus}>
                {browserCopy.recheck}
              </Button>
            }
          />
        )}

        {/* 就绪：浏览器能力随总开关自动激活，无需手动「开启」。直接展示动作流；
            浏览器未开时给登录提示，开着时给「关闭浏览器」（不再画没用的死按钮）。 */}
        {phase.kind === "ready" && (
          <div className="flex h-full min-h-0 flex-col">
            <div className="min-h-0 flex-1 overflow-y-auto">
              <BrowserActionStream
                rows={rows}
                feedVersion={feedVersion}
                running={running}
                emptyExtra={
                  isOpen ? undefined : headless ? (
                    <p className="max-w-[260px] text-[12px] text-foreground-muted">
                      {browserCopy.headlessHint}
                    </p>
                  ) : (
                    <div className="flex flex-col items-center gap-2">
                      <Button tone="primary" disabled={opening} onClick={doOpen}>
                        {opening ? browserCopy.openingBrowser : browserCopy.openBrowser}
                      </Button>
                      <p className="max-w-[260px] text-[12px] text-foreground-muted">
                        {browserCopy.loginHintBody}
                      </p>
                    </div>
                  )
                }
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
