import { readFileSync } from "node:fs";

const sessionPageSource = readFileSync("src/pages/session/SessionPage.tsx", "utf8");
const monitorPanelSource = readFileSync("src/components/session/SessionMonitorPanel.tsx", "utf8");

if (!sessionPageSource.includes("collapsedMonitorSessionId")) {
  throw new Error("Session monitor collapse state should be scoped by the current session id");
}

if (!sessionPageSource.includes("collapsedMonitorSessionId === detail.session.id")) {
  throw new Error("Session monitor should collapse only for the matching current session");
}

if (!sessionPageSource.includes("transition-[width,opacity]")) {
  throw new Error("Session monitor rail should collapse with a width/opacity transition");
}

if (sessionPageSource.includes("{!monitorCollapsed &&") || sessionPageSource.includes("{monitorCollapsed ? null")) {
  throw new Error("SessionMonitorPanel should stay mounted while the rail animates closed");
}

if (!monitorPanelSource.includes("onCollapse")) {
  throw new Error("SessionMonitorPanel should expose a collapse callback");
}

if (!sessionPageSource.includes("展开任务面板")) {
  throw new Error("Collapsed session view should render a header affordance to reopen task monitor");
}

const taskPanelToggleStart = sessionPageSource.indexOf(
  'aria-label="展开任务面板"',
);
if (taskPanelToggleStart === -1) {
  throw new Error("SessionPage should render a header expand button when the task panel is collapsed");
}
const taskPanelToggleSource = sessionPageSource.slice(
  taskPanelToggleStart,
  taskPanelToggleStart + 700,
);
if (!sessionPageSource.includes("{monitorCollapsed && (")) {
  throw new Error("SessionPage header task panel button should render only while collapsed");
}
if (!taskPanelToggleSource.includes("<PanelRightOpen")) {
  throw new Error("SessionPage should show the open icon when the task panel is collapsed");
}
if (taskPanelToggleSource.includes("<PanelRightClose")) {
  throw new Error("SessionPage header should not show the collapse icon while the task panel is expanded");
}
if (!monitorPanelSource.includes("<PanelRightClose")) {
  throw new Error("SessionMonitorPanel should own the collapse icon while expanded");
}
