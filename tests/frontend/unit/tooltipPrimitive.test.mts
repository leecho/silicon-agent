import { existsSync, readFileSync } from "node:fs";

const tooltipPath = "src/components/ui/Tooltip.tsx";

if (!existsSync(tooltipPath)) {
  throw new Error("Tooltip component should exist in src/components/ui/Tooltip.tsx");
}

const tooltipSource = readFileSync(tooltipPath, "utf8");
const uiIndexSource = readFileSync("src/components/ui/index.ts", "utf8");
const sidebarActionsSource = readFileSync("src/components/layout/SidebarTitlebarActions.tsx", "utf8");
const drawerSource = readFileSync("src/components/ui/Drawer.tsx", "utf8");
const assistantAnswerSource = readFileSync("src/components/session/AssistantAnswer.tsx", "utf8");
const selectSource = readFileSync("src/components/ui/Select.tsx", "utf8");

for (const required of [
  "export function Tooltip",
  "createPortal",
  'role="tooltip"',
  "tooltipRef",
  "clampTooltipPosition",
  "offsetWidth",
  "offsetHeight",
  "onMouseEnter",
  "onMouseLeave",
  "onFocus",
  "onBlur",
  "Escape",
  "aria-describedby"
]) {
  if (!tooltipSource.includes(required)) {
    throw new Error(`Tooltip component should include ${required}`);
  }
}

if (tooltipSource.includes('transform: rect.placement === "top" ? "translate(-50%, -100%)" : "translate(-50%, 0)"')) {
  throw new Error("Tooltip should not rely on translate(-50%) positioning because long content can overflow the viewport");
}

if (!uiIndexSource.includes('export { Tooltip } from "./Tooltip";')) {
  throw new Error("UI index should export Tooltip");
}

if (!sidebarActionsSource.includes('import { Tooltip } from "../ui"')) {
  throw new Error("Sidebar titlebar actions should use the shared Tooltip component");
}

if (sidebarActionsSource.includes("title=")) {
  throw new Error("Sidebar titlebar actions should not use native title tooltips");
}

if (!drawerSource.includes('content="关闭"') || drawerSource.includes('title="关闭"')) {
  throw new Error("Drawer close action should use Tooltip instead of native title");
}

if (
  !assistantAnswerSource.includes('content="复制"') ||
  !assistantAnswerSource.includes('content="复制成Markdown"') ||
  assistantAnswerSource.includes('title="复制"') ||
  assistantAnswerSource.includes('title="复制成Markdown"')
) {
  throw new Error("Assistant answer copy actions should use Tooltip instead of native title");
}

if (!selectSource.includes("tooltip") || selectSource.includes("title={title}")) {
  throw new Error("Select trigger hints should use a tooltip prop instead of native title");
}
