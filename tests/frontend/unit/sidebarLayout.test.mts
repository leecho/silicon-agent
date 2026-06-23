import {
  COLLAPSED_SIDEBAR_TITLEBAR_ACTIONS_CLASS_NAME,
  getSidebarLayoutState,
  getTitlebarLayoutState,
  SIDEBAR_TITLEBAR_ACTIONS_CLASS_NAME,
  SIDEBAR_TITLEBAR_BUTTON_CLASS_NAME,
  type SidebarMode,
} from "../../../src/components/layout/sidebarLayout.ts";

function assertEqual<T>(actual: T, expected: T, message: string) {
  if (actual !== expected) {
    throw new Error(`${message}: expected ${String(expected)}, got ${String(actual)}`);
  }
}

function assertLayout(
  mode: SidebarMode,
  previewOpen: boolean,
  expected: {
    gridColumns: string;
    sidebarVisible: boolean;
    sidebarOverlay: boolean;
    sidebarPinned: boolean;
  },
) {
  const state = getSidebarLayoutState(mode, previewOpen);
  assertEqual(state.gridColumns, expected.gridColumns, "gridColumns");
  assertEqual(state.sidebarVisible, expected.sidebarVisible, "sidebarVisible");
  assertEqual(state.sidebarOverlay, expected.sidebarOverlay, "sidebarOverlay");
  assertEqual(state.sidebarPinned, expected.sidebarPinned, "sidebarPinned");
}

assertLayout("pinned", false, {
  gridColumns: "264px minmax(0, 1fr)",
  sidebarVisible: true,
  sidebarOverlay: false,
  sidebarPinned: true,
});

assertLayout("collapsed", false, {
  gridColumns: "0px minmax(0, 1fr)",
  sidebarVisible: false,
  sidebarOverlay: false,
  sidebarPinned: false,
});

assertLayout("collapsed", true, {
  gridColumns: "0px minmax(0, 1fr)",
  sidebarVisible: false,
  sidebarOverlay: false,
  sidebarPinned: false,
});

if (!SIDEBAR_TITLEBAR_ACTIONS_CLASS_NAME.includes("absolute")) {
  throw new Error("sidebar titlebar actions must be positioned outside normal sidebar layout flow");
}

if (!SIDEBAR_TITLEBAR_ACTIONS_CLASS_NAME.includes("flex")) {
  throw new Error("sidebar titlebar actions should support multiple buttons");
}

if (!SIDEBAR_TITLEBAR_ACTIONS_CLASS_NAME.includes("top-1")) {
  throw new Error("sidebar titlebar actions should align with the top window controls");
}

if (!SIDEBAR_TITLEBAR_BUTTON_CLASS_NAME.includes("grid h-8 w-8")) {
  throw new Error("sidebar titlebar button should keep stable icon button dimensions");
}

if (!COLLAPSED_SIDEBAR_TITLEBAR_ACTIONS_CLASS_NAME.includes("left-[var(--titlebar-collapsed-actions-left)]")) {
  throw new Error("collapsed sidebar titlebar actions should use the platform titlebar safe area");
}

const macTitlebar = getTitlebarLayoutState("macos");
assertEqual(macTitlebar.collapsedActionsLeft, "90px", "macOS collapsed action safe area");
assertEqual(macTitlebar.collapsedSessionHeaderPaddingLeft, "204px", "macOS collapsed header padding");

const windowsTitlebar = getTitlebarLayoutState("windows");
assertEqual(windowsTitlebar.collapsedActionsLeft, "12px", "Windows collapsed action safe area");
assertEqual(windowsTitlebar.collapsedSessionHeaderPaddingLeft, "126px", "Windows collapsed header padding");

const linuxTitlebar = getTitlebarLayoutState("linux");
assertEqual(linuxTitlebar.collapsedActionsLeft, "12px", "Linux collapsed action safe area");
assertEqual(linuxTitlebar.collapsedSessionHeaderPaddingLeft, "126px", "Linux collapsed header padding");

if (windowsTitlebar.collapsedActionsLeft === macTitlebar.collapsedActionsLeft) {
  throw new Error("Windows should not reserve macOS traffic light space on the left");
}
