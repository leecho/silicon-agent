import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/teams/TeamsPage.tsx", "utf8");

for (const required of [
  "function TabButton",
  "border-b-2",
  "团队广场",
  "我的团队",
  "flex items-end justify-between",
  "flex items-end gap-2",
  "<TeamPlaza />",
  "ArrowUpRight",
  "TeamGrid",
  "MineTeamSearch",
  "GroupFilterBar",
  "filteredMineTeams",
  "搜索我的团队名称、分类或描述",
  "handleDeleteGroup",
  "messages.confirm",
  "deleteGroup(g.id, \"team\")",
  "TeamGroupDropdown",
  "DropdownMenu",
  "items: DropdownMenuEntry[]",
  "Badge tone={selectedGroup ? \"info\" : \"neutral\"}",
  "inline-flex max-w-[132px] shrink-0 rounded-full",
  "inline-flex max-w-full min-w-0 items-center gap-1",
  "ChevronDown",
  "grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3",
  "已启用",
]) {
  if (!source.includes(required)) {
    throw new Error(`TeamsPage should use the optimized tab/list layout: missing ${required}`);
  }
}

if (source.includes("inline-flex rounded-lg border border-border-subtle bg-surface p-0.5")) {
  throw new Error("TeamsPage should not use the old segmented tab style");
}

if (source.includes("function TeamList") || source.includes("<ul className=\"overflow-hidden rounded-lg border")) {
  throw new Error("Mine teams should use management cards instead of the old row list");
}

for (const removedFilter of [
  "type MineTeamFilter",
  "MineTeamFilters",
  "setMineFilter",
  "mineFilter",
  "{ id: \"user\", label: \"用户创建\" }",
  "{ id: \"imported\", label: \"导入\" }",
  "{ id: \"builtin\", label: \"内置\" }",
]) {
  if (source.includes(removedFilter)) {
    throw new Error(`Mine teams should use group filtering instead of legacy source chips: ${removedFilter}`);
  }
}

if (source.includes("GroupMoveSelect")) {
  throw new Error("Mine team group switching should use Badge + DropdownMenu, not the inline select");
}
