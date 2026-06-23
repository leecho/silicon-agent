import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/skills/SkillDetailDrawer.tsx", "utf8");

for (const required of [
  "type DetailPage = \"default\" | \"files\"",
  "setPage(\"default\")",
  "page === \"default\"",
  "技能说明",
  "detail.skill.description",
  "detail && page === \"default\"",
  "查看文件",
  "setPage(\"files\")",
  "detail && page === \"files\"",
  "技能文件",
  "返回详情",
  "setActiveFile(null)",
  "setPreview(null)",
  "grid min-h-0 grid-rows-[minmax(0,1fr)]",
  "overflow-y-auto",
  "stripSkillFrontmatterForDisplay",
  "displaySkillMd",
  "value={displaySkillMd}",
  "line.trim() === \"---\"",
  "collapsedDirs",
  "collectSkillFileDirs",
  "overflow-x-auto",
  "onToggleDir",
  "ChevronRight",
  "ChevronDown",
]) {
  if (!source.includes(required)) {
    throw new Error(`SkillDetailDrawer should use default/file pages: missing ${required}`);
  }
}

for (const legacy of [
  "import { Tabs }",
  "const tabItems",
  "<Tabs",
  "type Tab = \"md\" | \"files\"",
  "setTab",
  "uppercase tracking-wide",
  ">SKILL.md<",
  "value={detail.skillMd}",
]) {
  if (source.includes(legacy)) {
    throw new Error(`SkillDetailDrawer should not use legacy tabs: ${legacy}`);
  }
}
