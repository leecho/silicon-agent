import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/skills/SkillDetailDrawer.tsx", "utf8");

for (const required of [
  "function stripSkillFrontmatterForDisplay",
  "replace(/^\\uFEFF/, \"\")",
  "lines[0]?.trim() !== \"---\"",
  "line.trim() === \"---\"",
  "lines.slice(endIndex + 1)",
  "value={displaySkillMd}",
]) {
  if (!source.includes(required)) {
    throw new Error(`Skill detail should hide SKILL.md metadata frontmatter: missing ${required}`);
  }
}

if (source.includes("value={detail.skillMd}")) {
  throw new Error("Skill detail should render stripped markdown, not raw SKILL.md");
}
