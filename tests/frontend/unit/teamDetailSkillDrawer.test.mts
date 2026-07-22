import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/teams/TeamDetailDrawer.tsx", "utf8");

for (const token of [
  "import { SkillDetailDrawer } from \"../skills/SkillDetailDrawer\"",
  "const [selectedSkillId, setSelectedSkillId] = useState<string | null>(null)",
  "setSelectedSkillId(s.id)",
  "<SkillDetailDrawer skillId={selectedSkillId}",
]) {
  if (!source.includes(token)) {
    throw new Error(`TeamDetailDrawer should open team skill details in-place: missing ${token}`);
  }
}

if (!source.includes('title="查看技能详情"')) {
  throw new Error("TeamDetailDrawer skill rows should advertise opening skill details");
}
