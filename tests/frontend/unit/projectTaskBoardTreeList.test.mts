import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/projects/ProjectTaskBoard.tsx", "utf8");

if (!source.includes("<ListView tasks={allTasks}")) {
  throw new Error("ProjectTaskBoard list view should receive all project tasks so parent rows can be rendered");
}

if (!source.includes("const boardTasks = allTasks.filter((t) => t.parentTaskId)")) {
  throw new Error("ProjectTaskBoard should keep board view scoped to child leaf tasks");
}

for (const token of [
  "function buildTaskTree",
  "mainTasks",
  "subsByMain",
  "orphanTasks",
  "useMemo",
  "openIds",
  "toggleMain",
  "ChevronDown",
  "ChevronRight",
  "子任务",
  "尚未拆分子任务",
]) {
  if (!source.includes(token)) {
    throw new Error(`ProjectTaskBoard list view should render a parent/child tree: missing ${token}`);
  }
}

const listViewStart = source.indexOf("function ListView");
const listViewEnd = source.indexOf("\nfunction ", listViewStart + 1);
const listViewSource = source.slice(listViewStart, listViewEnd > 0 ? listViewEnd : undefined);

if (listViewSource.includes("tasks.map((t) =>")) {
  throw new Error("ProjectTaskBoard list view should not render a flat tasks.map list");
}

if (!listViewSource.includes("mainTasks.map((main) =>")) {
  throw new Error("ProjectTaskBoard list view should render top-level parent task rows");
}

if (!listViewSource.includes("subs.map((sub) =>")) {
  throw new Error("ProjectTaskBoard list view should render child task rows under each parent");
}
