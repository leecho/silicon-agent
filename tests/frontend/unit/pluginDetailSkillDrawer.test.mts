import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/plugins/PluginDetailDrawer.tsx", "utf8");

for (const token of [
  "import { SkillDetailDrawer } from \"../skills/SkillDetailDrawer\"",
  "const [selectedSkillId, setSelectedSkillId] = useState<string | null>(null)",
  "setSelectedSkillId(skill.id)",
  "<SkillDetailDrawer skillId={selectedSkillId}",
]) {
  if (!source.includes(token)) {
    throw new Error(`PluginDetailDrawer should open skill details in-place: missing ${token}`);
  }
}

for (const removed of ["enterDraftWithContent", "useSkillInDraft", "在新对话里用这个技能"]) {
  if (source.includes(removed)) {
    throw new Error(`PluginDetailDrawer should not send skill clicks to draft: found ${removed}`);
  }
}
