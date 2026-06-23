import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/skills/SkillsPage.tsx", "utf8");

for (const required of [
  "type SkillPageTab = \"plaza\" | \"mine\"",
  "type MineSkillFilter",
  "技能广场",
  "我的技能",
  "SkillPlaza",
  "SkillGrid",
  "MineSkillFilters",
  "filteredMineSkills",
  "搜索我的技能名称或描述",
  "mode === \"plaza\" && skill.userInvocable",
  "plaza action",
  "plazaGridClass",
  "plazaCardClass",
  "plazaIconClass",
  "plazaDescriptionClass",
  "grid grid-cols-1 items-start gap-3 sm:grid-cols-2 lg:grid-cols-3",
  "grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3",
  "group flex flex-col rounded-xl border border-border-subtle bg-surface p-3",
  "grid h-9 w-9 shrink-0 place-items-center",
]) {
  if (!source.includes(required)) {
    throw new Error(`SkillsPage should use plaza/mine card layout: missing ${required}`);
  }
}

if (source.includes("mode === \"mine\" ?") && source.includes("mt-auto flex justify-end pt-3")) {
  throw new Error("Skill plaza use action should live in the title row, not the card footer");
}

if (source.includes("group flex min-h-[168px] flex-col rounded-xl border border-border-subtle bg-surface p-4") && !source.includes("plazaCardClass")) {
  throw new Error("Skill plaza cards should not share the tall mine-card shell");
}

for (const legacy of [
  "setTab(\"builtin\")",
  "setTab(\"user\")",
  "tab === \"builtin\"",
  "tab === \"user\"",
  "function SkillList",
  "<ul className=\"overflow-hidden rounded-lg border",
]) {
  if (source.includes(legacy)) {
    throw new Error(`SkillsPage should not keep legacy split/list layout: ${legacy}`);
  }
}
