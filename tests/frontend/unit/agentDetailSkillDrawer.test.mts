import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/agents/AgentDetailDrawer.tsx", "utf8");

for (const token of [
  "import { SkillDetailDrawer } from \"../skills/SkillDetailDrawer\"",
  "const [selectedSkillId, setSelectedSkillId] = useState<string | null>(null)",
  "setSelectedSkillId(s.id)",
  "<SkillDetailDrawer skillId={selectedSkillId}",
]) {
  if (!source.includes(token)) {
    throw new Error(`AgentDetailDrawer should open carried skill details in-place: missing ${token}`);
  }
}

if (!source.includes('title="查看技能详情"')) {
  throw new Error("AgentDetailDrawer skill rows should advertise opening skill details");
}
