import { useRef } from "react";
import { Plus, Settings as SettingsIcon } from "lucide-react";
import { primaryNavItems, type AppSection } from "../../appNavigation";
import { WindowDragRegion } from "./WindowDragRegion";
import { SessionManager } from "./SessionManager";
import { useSession } from "../session/SessionProvider";
import {
  SIDEBAR_TITLEBAR_ACTIONS_CLASS_NAME,
  SIDEBAR_WIDTH_PX,
  type SidebarMode,
} from "./sidebarLayout";
import { SidebarTitlebarActions } from "./SidebarTitlebarActions";
import { Tooltip } from "../ui";

export function Sidebar({
  activeSection,
  canBack,
  canForward,
  mode,
  onBack,
  onForward,
  onNavigateDraft,
  onNavigateSession,
  onSearch,
  onSelectSection,
  onToggleMode,
}: {
  activeSection: AppSection;
  canBack?: boolean;
  canForward?: boolean;
  mode: SidebarMode;
  onBack?: () => void;
  onForward?: () => void;
  onNavigateDraft: (draftId: string) => void;
  onNavigateSession: (sessionId: string) => void;
  onSearch?: () => void;
  onSelectSection: (section: AppSection) => void;
  onToggleMode: () => void;
}) {
  // 会话不再是导航项——点列表任意会话即在会话区，会话切换由 SessionManager 承担。
  const topNavItems = primaryNavItems.filter(
    (item) => item.id !== "home" && item.id !== "session",
  );
  const { enterDraft } = useSession();
  const sidebarRef = useRef<HTMLElement | null>(null);

  return (
    <aside
      ref={sidebarRef}
      className="relative flex h-screen min-h-0 flex-col gap-2 overflow-hidden border-r border-border-subtle bg-surface px-3 pt-9 pb-2 text-card-foreground"
      style={{ width: SIDEBAR_WIDTH_PX }}
    >
      <WindowDragRegion />

      <SidebarTitlebarActions
        canBack={canBack}
        canForward={canForward}
        className={SIDEBAR_TITLEBAR_ACTIONS_CLASS_NAME}
        homeActive={activeSection === "home"}
        mode={mode}
        onBack={onBack}
        onForward={onForward}
        onHome={() => onSelectSection("home")}
        onNewTask={enterDraft}
        onSearch={onSearch}
        onToggleMode={onToggleMode}
      />

      <button
        type="button"
        onClick={() => enterDraft()}
        className="flex shrink-0 items-center gap-3 rounded-[9px] px-2 py-2 text-left text-sm font-semibold text-foreground transition hover:bg-accent hover:text-accent-foreground"
      >
        <Plus className="h-[17px] w-[17px] shrink-0" aria-hidden="true" />
        <span>新任务</span>
      </button>

      <nav className="flex shrink-0 flex-col" aria-label="Primary">
        {topNavItems.map((item) => {
          const Icon = item.icon;
          const active = item.id === activeSection;
          return (
            <button
              aria-current={active ? "page" : undefined}
              className={`flex items-center gap-3 rounded-[9px] px-2 py-2 text-left text-[13px] transition ${
                active
                  ? "bg-accent font-semibold text-accent-foreground"
                  : "text-foreground hover:bg-accent hover:text-accent-foreground"
              }`}
              key={item.id}
              type="button"
              onClick={() => onSelectSection(item.id)}
            >
              <Icon className="h-[17px] w-[17px] shrink-0 text-foreground" aria-hidden="true" />
              <span>{item.label}</span>
            </button>
          );
        })}
      </nav>

      <SessionManager
        onNavigateDraft={onNavigateDraft}
        onNavigateSession={onNavigateSession}
      />


      <Tooltip content="账户设置">
        <div className="mt-auto flex shrink-0 cursor-pointer items-center gap-2.5 rounded-lg px-1 py-1.5 hover:bg-accent" onClick={() => onSelectSection("settings")}>
          <div className="grid h-9 w-9 place-items-center rounded-full bg-muted font-bold text-foreground">S</div>
          <div className="min-w-0 leading-tight">
            <div className="truncate text-[13px] font-bold text-foreground">SiliconAgent</div>
            <div className="truncate text-[11px] text-foreground-muted">Agent runtime</div>
          </div>
          <button
            className="ml-auto grid h-8 w-8 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
            type="button"
            aria-label="设置"
            onClick={(e) => { e.stopPropagation(); onSelectSection("settings"); }}
          >
            <SettingsIcon className="h-[18px] w-[18px]" aria-hidden="true" />
          </button>
        </div>
      </Tooltip>
    </aside>
  );
}
