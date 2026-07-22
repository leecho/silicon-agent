import type { ReactNode } from "react";
import { PanelRightClose } from "lucide-react";
import { Tooltip } from "../ui/Tooltip";

/** 右侧任务面板的 tab 标识：监控 / 工作空间 / 浏览器 / 桌面。 */
export type SidePanelTab = "monitor" | "workspace" | "browser" | "computer";

export interface SidePanelTabDef {
  key: SidePanelTab;
  label: string;
  icon: ReactNode;
}

/**
 * 右侧任务面板的「tab 壳」：顶部一条 tab（监控/浏览器/桌面）+ 收起按钮，下面渲染当前 tab 正文。
 * 把原先三块并排的独立侧栏（任务监控 + 桌面 + 浏览器）合并为同一块、按 tab 切换，
 * 不再各自挤占布局；各面板正文以 embedded 模式渲染（去掉各自的标题头，标题由 tab 承担）。
 */
export function SessionSidePanel({
  tab,
  tabs,
  onTab,
  onCollapse,
  children,
}: {
  tab: SidePanelTab;
  tabs: SidePanelTabDef[];
  onTab: (t: SidePanelTab) => void;
  onCollapse: () => void;
  children: ReactNode;
}) {
  return (
    <div className="flex h-full min-h-0 w-full flex-col border-l border-border-subtle text-card-foreground">
      <div className="flex items-center justify-between gap-1 border-b border-border-subtle px-2 py-3">
        <div className="no-scrollbar flex min-w-0 items-center gap-1 overflow-x-auto">
          {tabs.map((t) => {
            const active = t.key === tab;
            return (
              <button
                key={t.key}
                type="button"
                aria-pressed={active}
                onClick={() => onTab(t.key)}
                className={`flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-xs font-medium transition ${
                  active
                    ? "bg-accent text-foreground"
                    : "text-foreground-secondary hover:bg-accent/60 hover:text-foreground"
                }`}
              >
                {t.icon}
                <span className="truncate">{t.label}</span>
              </button>
            );
          })}
        </div>
        <Tooltip content="收起任务面板">
          <button
            type="button"
            aria-label="收起任务面板"
            className="grid h-7 w-7 shrink-0 place-items-center rounded-md text-foreground-secondary transition hover:bg-accent hover:text-foreground"
            onClick={onCollapse}
          >
            <PanelRightClose className="h-[14px] w-[14px]" aria-hidden="true" />
          </button>
        </Tooltip>
      </div>
      <div className="min-h-0 flex-1 overflow-hidden">{children}</div>
    </div>
  );
}
