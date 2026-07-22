import { readFileSync } from "node:fs";

const viewSource = readFileSync("src/pages/agents/AgentView.tsx", "utf8");
const overviewSource = readFileSync("src/pages/agents/AgentOverview.tsx", "utf8");
const drawersSource = readFileSync("src/pages/agents/AgentViewDrawers.tsx", "utf8");

for (const required of [
  "listStandaloneExperts",
  "sourceExpertDisplayName",
  "AgentSourceExpertSwitchDrawer",
  "const [sourceExpertOpen, setSourceExpertOpen] = useState(false)",
  "onSwitchSourceExpert={() => setSourceExpertOpen(true)}",
  "sourceExpertDisplayName={sourceExpertDisplayName}",
  "<AgentSourceExpertSwitchDrawer",
]) {
  if (!viewSource.includes(required)) {
    throw new Error(`AgentView should wire source expert switching from the detail page: missing ${required}`);
  }
}

for (const required of [
  "sourceExpertDisplayName",
  "onSwitchSourceExpert",
  "专家：{sourceExpertDisplayName}",
  "切换",
]) {
  if (!overviewSource.includes(required)) {
    throw new Error(`AgentOverview should expose the source expert switch on the skill card: missing ${required}`);
  }
}

if (overviewSource.includes("来源专家: {agent.sourceExpertId")) {
  throw new Error("AgentOverview should not show the source expert id in the header.");
}

for (const required of [
  "export function AgentSourceExpertSwitchDrawer",
  "ExpertPickerDialog",
  "sourceExpertId",
  "await updateAgent({ ...agent, sourceExpertId: nextSourceExpert })",
  "onSaved()",
]) {
  if (!drawersSource.includes(required)) {
    throw new Error(`AgentViewDrawers should save source expert changes through updateAgent: missing ${required}`);
  }
}
