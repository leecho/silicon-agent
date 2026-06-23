import { readFileSync } from "node:fs";

const addMenuSource = readFileSync("src/pages/session/composer/AddMenu.tsx", "utf8");
const composerSource = readFileSync("src/components/session/Composer.tsx", "utf8");
const sessionPageSource = readFileSync("src/pages/session/SessionPage.tsx", "utf8");

if (!addMenuSource.includes("planMode?: boolean")) {
  throw new Error("AddMenu should accept planMode so the + menu can render plan switch state");
}

if (!addMenuSource.includes("onTogglePlan?: () => void")) {
  throw new Error("AddMenu should accept onTogglePlan so the + menu can toggle plan mode");
}

if (!addMenuSource.includes('label: "计划模式"')) {
  throw new Error("AddMenu should expose plan mode as a + menu item");
}

if (!addMenuSource.includes("PlanModeSwitch")) {
  throw new Error("AddMenu should render plan mode with an inline switch");
}

if (!composerSource.includes("planMode={planMode}")) {
  throw new Error("Composer should pass planMode into AddMenu");
}

if (!composerSource.includes("onTogglePlan={onTogglePlan}")) {
  throw new Error("Composer should pass onTogglePlan into AddMenu");
}

if (!composerSource.includes("planMode && onTogglePlan")) {
  throw new Error("Composer should render the toolbar plan chip only while plan mode is active");
}

if (!composerSource.includes('aria-label="关闭计划模式"')) {
  throw new Error("Composer plan chip should include a delete control");
}

const planChipStart = composerSource.indexOf("{planMode && onTogglePlan && (");
const planChipEnd = composerSource.indexOf('<div className="flex-1"', planChipStart);
if (planChipStart < 0 || planChipEnd <= planChipStart) {
  throw new Error("Composer should render a scoped plan chip before the toolbar spacer");
}
const planChipSource = composerSource.slice(planChipStart, planChipEnd);
if (!planChipSource.includes("hover:bg-accent")) {
  throw new Error("Composer plan chip should only show its background on hover");
}
if (!planChipSource.includes("rounded-md px-2 py-1.5 text-xs text-foreground-secondary")) {
  throw new Error("Composer plan chip should use the same lightweight style as other toolbar tools");
}
if (planChipSource.includes("rounded-full bg-accent") || planChipSource.includes("rounded-full bg-muted")) {
  throw new Error("Composer plan chip should not keep the old persistent pill background");
}
if (!planChipSource.includes("group/plan-chip")) {
  throw new Error("Composer plan chip should use a group hover state for icon swapping");
}
if (!planChipSource.includes("group-hover/plan-chip:hidden")) {
  throw new Error("Composer plan icon should be visible by default and hidden on chip hover");
}
if (!planChipSource.includes("hidden h-3.5 w-3.5") || !planChipSource.includes("group-hover/plan-chip:block")) {
  throw new Error("Composer close icon should be hidden by default and visible on chip hover");
}
if (!planChipSource.includes('content="计划模式：先只读调研、提交计划等你批准后再执行"')) {
  throw new Error("Composer plan chip should explain what plan mode does");
}
if (!planChipSource.includes("cursor-default")) {
  throw new Error("Composer plan chip should use the default cursor over its text label");
}

const toolbarPlanStart = composerSource.indexOf("{onTogglePlan && (");
const toolbarPlanEnd = composerSource.indexOf("{onPickRole &&", toolbarPlanStart);
if (toolbarPlanStart >= 0 && toolbarPlanEnd > toolbarPlanStart) {
  const legacyToolbarPlan = composerSource.slice(toolbarPlanStart, toolbarPlanEnd);
  if (legacyToolbarPlan.includes("<button") && legacyToolbarPlan.includes("计划模式")) {
    throw new Error("Composer should not keep the standalone toolbar plan toggle button");
  }
}

if (
  sessionPageSource.includes("已开启计划模式") ||
  sessionPageSource.includes("已关闭计划模式") ||
  sessionPageSource.includes('id: "plan-" + Date.now()')
) {
  throw new Error("SessionPage should not append plan-mode toggle messages to the feed");
}
