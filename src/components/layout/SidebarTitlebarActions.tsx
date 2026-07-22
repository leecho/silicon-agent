import { forwardRef } from "react";
import {
  ArrowLeft,
  ArrowRight,
  Home,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Search,
} from "lucide-react";
import { Tooltip } from "../ui";
import { SIDEBAR_TITLEBAR_BUTTON_CLASS_NAME, type SidebarMode } from "./sidebarLayout";

interface SidebarTitlebarActionsProps {
  canBack?: boolean;
  canForward?: boolean;
  className: string;
  homeActive?: boolean;
  mode: SidebarMode;
  onBack?: () => void;
  onForward?: () => void;
  onHome?: () => void;
  onNewTask?: () => void;
  onSearch?: () => void;
  onToggleMode: () => void;
}

export const SidebarTitlebarActions = forwardRef<HTMLDivElement, SidebarTitlebarActionsProps>(
  function SidebarTitlebarActions({
    canBack = false,
    canForward = false,
    className,
    homeActive = false,
    mode,
    onBack,
    onForward,
    onHome,
    onNewTask,
    onSearch,
    onToggleMode,
  }, ref) {
    const collapsed = mode === "collapsed";

    return (
      <div ref={ref} className={className}>
        <Tooltip content={collapsed ? "展开侧边栏" : "收起侧边栏"}>
          <button
            className={SIDEBAR_TITLEBAR_BUTTON_CLASS_NAME}
            type="button"
            aria-label={collapsed ? "展开侧边栏" : "收起侧边栏"}
            onClick={onToggleMode}
          >
            {collapsed ? (
              <PanelLeftOpen className="h-[14px] w-[14px]" aria-hidden="true" />
            ) : (
              <PanelLeftClose className="h-[14px] w-[14px]" aria-hidden="true" />
            )}
          </button>
        </Tooltip>
        <Tooltip content="首页">
          <button
            className={`${SIDEBAR_TITLEBAR_BUTTON_CLASS_NAME} ${
              homeActive ? "bg-accent text-accent-foreground" : ""
            }`}
            type="button"
            aria-label="首页"
            onClick={onHome}
          >
            <Home className="h-[14px] w-[14px]" aria-hidden="true" />
          </button>
        </Tooltip>
        <Tooltip content="后退">
          <button
            className={SIDEBAR_TITLEBAR_BUTTON_CLASS_NAME}
            type="button"
            aria-label="后退"
            disabled={!canBack}
            onClick={onBack}
          >
            <ArrowLeft className="h-[14px] w-[14px]" aria-hidden="true" />
          </button>
        </Tooltip>
        <Tooltip content="前进">
          <button
            className={SIDEBAR_TITLEBAR_BUTTON_CLASS_NAME}
            type="button"
            aria-label="前进"
            disabled={!canForward}
            onClick={onForward}
          >
            <ArrowRight className="h-[14px] w-[14px]" aria-hidden="true" />
          </button>
        </Tooltip>
        <Tooltip content="搜索">
          <button
            className={SIDEBAR_TITLEBAR_BUTTON_CLASS_NAME}
            type="button"
            aria-label="搜索"
            onClick={onSearch}
          >
            <Search className="h-[14px] w-[14px]" aria-hidden="true" />
          </button>
        </Tooltip>
        {collapsed && (
          <Tooltip content="新会话">
            <button
              className={SIDEBAR_TITLEBAR_BUTTON_CLASS_NAME}
              type="button"
              aria-label="新会话"
              onClick={onNewTask}
            >
              <Plus className="h-[14px] w-[14px]" aria-hidden="true" />
            </button>
          </Tooltip>
        )}
      </div>
    );
  },
);
