import { readFileSync } from "node:fs";

const pageSource = readFileSync("src/pages/agents/AgentsPage.tsx", "utf8");
const listSource = readFileSync("src/pages/agents/AgentList.tsx", "utf8");
const viewSource = readFileSync("src/pages/agents/AgentView.tsx", "utf8");
const overviewSource = readFileSync("src/pages/agents/AgentOverview.tsx", "utf8");
const sessionListSource = readFileSync("src/pages/agents/AgentSessionList.tsx", "utf8");
const artifactListSource = readFileSync("src/pages/agents/AgentArtifactList.tsx", "utf8");
const usageSource = readFileSync("src/pages/agents/AgentUsage.tsx", "utf8");
const drawersSource = readFileSync("src/pages/agents/AgentViewDrawers.tsx", "utf8");
const builderSource = readFileSync("src/pages/agents/AgentBuilderDrawer.tsx", "utf8");
const emojiPickerSource = readFileSync("src/components/ui/EmojiPicker.tsx", "utf8");
const uiIndexSource = readFileSync("src/components/ui/index.ts", "utf8");
const apiSource = readFileSync("src/api.ts", "utf8");
const agentCommandsSource = readFileSync("src-tauri/src/commands/agent.rs", "utf8");
const libSource = readFileSync("src-tauri/src/lib.rs", "utf8");

for (const required of [
  "const open = agents.find((a) => a.id === openId) ?? null",
  "<AgentView",
  "<AgentList",
  "onCreated={(a) =>",
]) {
  if (!pageSource.includes(required)) {
    throw new Error(`AgentsPage should keep the Projects-style list/view shell: missing ${required}`);
  }
}

for (const required of [
  "grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3",
  "AgentBuilderDrawer",
  "group flex",
  "onClick={() => onOpen(agent.id)}",
  "group-hover:opacity-100",
  "void onDelete(agent)",
]) {
  if (!listSource.includes(required)) {
    throw new Error(`AgentList should follow ProjectList's simple card layout: missing ${required}`);
  }
}

for (const removed of [
  "useMemo",
  "Search",
  "SummaryPill",
  "AgentStatusPill",
  "Switch",
  "toggleAgent",
  "enterDraftWithAgent",
  "搜索智能体名称、职业或人设",
  "没有匹配的智能体",
  "专属工作目录",
  "agent.tools.length",
  "agent.instructions",
]) {
  if (listSource.includes(removed)) {
    throw new Error(`AgentList should stay as simple as ProjectList and remove: ${removed}`);
  }
}

for (const required of [
  "type AgentViewMode",
  "const crumb",
  "\"tasks\"",
  "\"artifacts\"",
  "\"chat\"",
  "\"usage\"",
  "\"skills\"",
  "AgentOverview",
  "AgentSessionList",
  "AgentUsage",
  "ProjectTaskBoard",
  "AgentArtifactList",
  "SkillDetailDrawer",
  "listAgentSessions",
  "listAgentTasks",
  "listAgentArtifacts",
  "listAgentSkills",
  "listSoulVersions",
  "getAgentUsage",
  "setEvolutionEnabled",
  "AgentInstructionsViewDrawer",
  "AgentIdentityAnchorViewDrawer",
  "AgentIdentityEditDrawer",
  "AgentIdentityAnchorEditDrawer",
  "AgentSoulEditDrawer",
  "from \"./AgentOverview\"",
  "from \"./AgentSessionList\"",
  "from \"./AgentArtifactList\"",
  "from \"./AgentUsage\"",
  "from \"./AgentViewDrawers\"",
  "from \"../skills/SkillDetailDrawer\"",
]) {
  if (!viewSource.includes(required)) {
    throw new Error(`AgentView should use a project-like overview with drawers: missing ${required}`);
  }
}

for (const required of [
  "const [pendingSoulProposalCount, setPendingSoulProposalCount]",
  "versions.filter((v) => v.status === \"pending\").length",
  "pendingSoulProposalCount={pendingSoulProposalCount}",
  "onSetEvolutionEnabled={handleSetEvolutionEnabled}",
  "onEditIdentityAnchor={() => setIdentityAnchorEditOpen(true)}",
  "onEditSoul={() => setSoulEditOpen(true)}",
  "onViewIdentityAnchor={() => setIdentityAnchorViewOpen(true)}",
  "onGoSkills={() => setView(\"skills\")}",
  "agentSkillCount={agentSkills.length}",
  "AgentSkillList",
]) {
  if (!viewSource.includes(required)) {
    throw new Error(`AgentView should load SOUL evolution proposal state for the overview: missing ${required}`);
  }
}

for (const removedDefinition of [
  "function AgentOverview",
  "function AgentSessionList",
  "function AgentArtifactList",
  "function AgentUsage",
  "function AgentIdentityEditDrawer",
  "function AgentInstructionsViewDrawer",
  "function AgentInstructionsEditDrawer",
]) {
  if (viewSource.includes(removedDefinition)) {
    throw new Error(`AgentView should compose extracted components instead of defining ${removedDefinition}`);
  }
}

for (const [label, source, required] of [
  ["AgentOverview", overviewSource, "export function AgentOverview"],
  ["AgentSessionList", sessionListSource, "export function AgentSessionList"],
  ["AgentArtifactList", artifactListSource, "export function AgentArtifactList"],
  ["AgentUsage", usageSource, "export function AgentUsage"],
  ["AgentViewDrawers", drawersSource, "export function AgentIdentityEditDrawer"],
] as const) {
  if (!source.includes(required)) {
    throw new Error(`${label} should expose the extracted AgentView component: missing ${required}`);
  }
}

for (const required of [
  "查看人设",
  "工作目录",
  "Tooltip content=\"点击打开工作目录\"",
  "onClick={onOpenWorkspace}",
  "ClipboardList",
  "MessagesSquare",
  "h-[360px] flex-col rounded-xl border border-border-subtle bg-surface p-4 lg:col-span-2",
  "min-h-0 flex-1 overflow-auto",
  "grid grid-cols-1 gap-1 sm:grid-cols-2",
  "查看全部",
  "onClick={onGoChat}",
  "专属技能",
  "agentSkillCount",
  "onGoSkills",
  "身份",
  "人格",
  "pendingSoulProposalCount",
  "onSetEvolutionEnabled",
  "agent.identity?.trim()",
  "agent.instructions?.trim()",
  "onViewIdentityAnchor",
  "onEditIdentityAnchor",
  "onEditSoul",
  "space-y-2",
  "允许自我演化",
  "开启后会在攒够新经历时提出人格更新，提案需批准后生效。",
]) {
  if (!overviewSource.includes(required)) {
    throw new Error(`AgentOverview should own the overview presentation copy: missing ${required}`);
  }
}

for (const removed of [
  "onOpenSession: (id: string) => void",
  "sessions.slice(0, 5)",
  "onClick={() => onOpenSession(session.id)}",
  "<Sparkles className=\"h-4 w-4 text-foreground-secondary\" aria-hidden=\"true\" />\n              自我演化",
  "md:grid-cols-[0.9fr_1.1fr]",
  "onClick={onEditInstructions}",
  "人格</div>\n                  <button",
  "onClick={onViewInstructions}>\n                {agent.identity?.trim()",
]) {
  if (overviewSource.includes(removed)) {
    throw new Error(`AgentOverview session summary should be fixed-height and route through 查看全部: ${removed}`);
  }
}

if (!drawersSource.includes("export function AgentIdentityEditDrawer")) {
  throw new Error("AgentViewDrawers should own the identity edit drawer.");
}

for (const required of [
  "export function AgentIdentityAnchorViewDrawer",
  "export function AgentIdentityAnchorEditDrawer",
  "export function AgentSoulEditDrawer",
  "export function AgentEvolutionDrawer",
  "setIdentity(agent.identity)",
  "await updateAgent({ ...agent, identity })",
  "setInstructions(agent.instructions)",
  "await updateAgent({ ...agent, instructions })",
  "const [soulVersions, setSoulVersions]",
  "const [selectedSoulVersionId, setSelectedSoulVersionId]",
  "const selectedSoulVersion =",
  "setSelectedSoulVersionId(active?.id ?? versions[0]?.id ?? \"current\")",
  "import { Select } from \"../../components/ui/Select\"",
  "SOUL 版本",
  "const soulVersionOptions =",
  "<Select",
  "value={selectedSoulVersionId}",
  "onChange={setSelectedSoulVersionId}",
  "selectedSoulVersion?.soul ?? agent.instructions",
  "演化提案",
  "待批准提案（{pending.length}）",
]) {
  if (!drawersSource.includes(required)) {
    throw new Error(`AgentViewDrawers should provide separate IDENTITY and SOUL editors: missing ${required}`);
  }
}

for (const removed of [
  "setEvolutionEnabled",
  "rollbackSoulVersion",
  "允许自我演化",
  "SOUL 版本史",
  "回滚到此版本",
  "VersionRow",
  "<select",
  "</select>",
]) {
  if (drawersSource.includes(removed)) {
    throw new Error(`AgentEvolutionDrawer should be a proposal-only drawer and remove: ${removed}`);
  }
}

for (const required of [
  "pickDirectory",
  "workingDir",
  "工作目录",
  "当前使用智能体默认工作目录",
  "EmojiPicker",
]) {
  if (!drawersSource.includes(required)) {
    throw new Error(`AgentViewDrawers identity editor should support workspace and emoji picking: missing ${required}`);
  }
}

if (!builderSource.includes("EmojiPicker")) {
  throw new Error("AgentBuilderDrawer should reuse the common EmojiPicker component.");
}

for (const required of [
  "export const DEFAULT_AVATAR_EMOJIS",
  "export function EmojiPicker",
  "DropdownMenu",
  "grid grid-cols-8 gap-1",
  "min-w-20",
]) {
  if (!emojiPickerSource.includes(required)) {
    throw new Error(`EmojiPicker should be a reusable dropdown picker: missing ${required}`);
  }
}

for (const removed of [
  "<input",
  "placeholder",
  "选择\n",
]) {
  if (emojiPickerSource.includes(removed)) {
    throw new Error(`EmojiPicker should be a compact dropdown box without a separate text input: ${removed}`);
  }
}

if (!uiIndexSource.includes("EmojiPicker")) {
  throw new Error("components/ui should export EmojiPicker for reuse.");
}

for (const [label, source, required] of [
  ["api", apiSource, "openAgentWorkspace"],
  ["api", apiSource, "listAgentSkills"],
  ["agent command", agentCommandsSource, "open_agent_workspace"],
  ["agent command", agentCommandsSource, "list_agent_skills"],
  ["command registry", libSource, "commands::open_agent_workspace"],
  ["command registry", libSource, "commands::list_agent_skills"],
] as const) {
  if (!source.includes(required)) {
    throw new Error(`Agent workspace opening should be wired through backend command: missing ${required} in ${label}`);
  }
}

for (const removed of [
  "usageTotals.input",
  "usageTotals.output",
  "usageTotals.calls",
]) {
  if (overviewSource.includes(removed)) {
    throw new Error(`AgentOverview usage card should match ProjectHome and only show total usage: ${removed}`);
  }
}

if (viewSource.includes("min-h-[320px] resize-y font-mono")) {
  throw new Error("AgentView should not present the primary detail screen as one large edit textarea.");
}

for (const removed of [
  "运行状态",
  "Switch",
  "toggling",
  "onToggle",
  "技能引用",
  "模型档位",
  "agent.tools.length",
]) {
  if (viewSource.includes(removed)) {
    throw new Error(`AgentView should keep workspace in the header and remove the runtime status block: ${removed}`);
  }
}
