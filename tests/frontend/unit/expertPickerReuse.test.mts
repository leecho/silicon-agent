import { readFileSync } from "node:fs";

const pickerSource = readFileSync("src/components/experts/ExpertPickerDialog.tsx", "utf8");
const memberPickerSource = readFileSync("src/pages/projects/MemberPicker.tsx", "utf8");
const agentBuilderSource = readFileSync("src/pages/agents/AgentBuilderDrawer.tsx", "utf8");

for (const required of [
  "export function ExpertPickerDialog",
  "selectionMode",
  "\"single\" | \"multiple\"",
  "搜索专家（名称/职业）",
  "onConfirm",
]) {
  if (!pickerSource.includes(required)) {
    throw new Error(`ExpertPickerDialog should provide shared expert picking UI: missing ${required}`);
  }
}

for (const required of [
  "import { ExpertPickerDialog } from \"../../components/experts/ExpertPickerDialog\"",
  "<ExpertPickerDialog",
  "selectionMode=\"multiple\"",
]) {
  if (!memberPickerSource.includes(required)) {
    throw new Error(`Project MemberPicker should reuse shared ExpertPickerDialog: missing ${required}`);
  }
}

for (const required of [
  "instructionImportOpen",
  "从专家导入",
  "handleImportPrompt",
  "getExpertDetail(expert.id)",
  "setSourceExpert(expert.name)",
  "selectionMode=\"single\"",
]) {
  if (!agentBuilderSource.includes(required)) {
    throw new Error(`Agent builder should import prompts from an expert inside persona editing: missing ${required}`);
  }
}

if (agentBuilderSource.includes("从哪个专家播种") || agentBuilderSource.includes("<select")) {
  throw new Error("Agent builder should not use a top-level expert select input.");
}
