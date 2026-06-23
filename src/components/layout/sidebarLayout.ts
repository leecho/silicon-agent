import type { AppPlatform } from "../../api";

export type SidebarMode = "pinned" | "collapsed";

export interface SidebarLayoutState {
  gridColumns: string;
  sidebarVisible: boolean;
  sidebarOverlay: boolean;
  sidebarPinned: boolean;
}

export const SIDEBAR_TITLEBAR_ACTIONS_CLASS_NAME =
  "absolute right-3 top-1 z-20 flex h-8 items-center gap-1";

export const COLLAPSED_SIDEBAR_TITLEBAR_ACTIONS_CLASS_NAME =
  "absolute left-[var(--titlebar-collapsed-actions-left)] top-1 z-20 flex h-8 items-center gap-0.5";

export const SIDEBAR_TITLEBAR_BUTTON_CLASS_NAME =
  "grid h-8 w-8 shrink-0 place-items-center rounded-md text-foreground-secondary transition hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-35";

export const SIDEBAR_WIDTH_PX = 288;

export interface TitlebarLayoutState {
  collapsedActionsLeft: string;
  collapsedContentInsetFallback: string;
}

export function getTitlebarLayoutState(platform: AppPlatform): TitlebarLayoutState {
  const leftWindowControlSafeArea = platform === "macos" ? 90 : 12;

  return {
    collapsedActionsLeft: `${leftWindowControlSafeArea}px`,
    collapsedContentInsetFallback: `${leftWindowControlSafeArea + 12}px`,
  };
}

export function getSidebarLayoutState(
  mode: SidebarMode,
  _previewOpen: boolean,
): SidebarLayoutState {
  const sidebarPinned = mode === "pinned";

  return {
    gridColumns: `${sidebarPinned ? SIDEBAR_WIDTH_PX : 0}px minmax(0, 1fr)`,
    sidebarVisible: sidebarPinned,
    sidebarOverlay: false,
    sidebarPinned,
  };
}
