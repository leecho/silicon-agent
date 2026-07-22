import { existsSync, readFileSync } from "node:fs";

const agentSessionsPath = "src/components/layout/session-manager/AgentSessions.tsx";
if (!existsSync(agentSessionsPath)) {
  throw new Error("SessionManager should add an AgentSessions component");
}

const agentSessionActionMenuPath = "src/components/layout/session-manager/AgentSessionActionMenu.tsx";
if (!existsSync(agentSessionActionMenuPath)) {
  throw new Error("Agent session rows should have a dedicated action menu");
}

const appSource = readFileSync("src/App.tsx", "utf8");
const agentSessions = readFileSync(agentSessionsPath, "utf8");
const actionMenu = readFileSync(agentSessionActionMenuPath, "utf8");
const agentsPage = readFileSync("src/pages/agents/AgentsPage.tsx", "utf8");
const navigation = readFileSync("src/hooks/useAppNavigation.ts", "utf8");
const normalSessions = readFileSync("src/components/layout/session-manager/NormalSessions.tsx", "utf8");
const sessionManager = readFileSync("src/components/layout/SessionManager.tsx", "utf8");
const sidebar = readFileSync("src/components/layout/Sidebar.tsx", "utf8");

for (const token of [
  "AgentSessions",
  "listAgents",
  "listAgentSessions",
  "type Agent",
  'from "./SessionRows"',
  "<GroupRow",
  "<ItemRow",
  "byPinnedThenUpdated",
  "agent.displayName ?? agent.name",
  "onNewAgentSession",
  "onOpenAgentSessionMenu",
  'aria-label="新增智能体"',
  'aria-label="智能体列表"',
  'aria-label={`查看智能体：${agent.displayName ?? agent.name}`}',
  'aria-label={`新增智能体会话：${agent.displayName ?? agent.name}`}',
  "group-hover:opacity-100",
  "group-focus-within:opacity-100",
]) {
  if (!agentSessions.includes(token)) {
    throw new Error(`AgentSessions should include ${token}`);
  }
}

for (const forbidden of ["SessionTreeContent", "SessionTreeNode", "SessionManagerTemplate", "function SectionTitle", "<SectionTitle"]) {
  if (agentSessions.includes(forbidden)) {
    throw new Error(`AgentSessions should not use legacy tree/template/title abstractions: found ${forbidden}`);
  }
}

if (agentSessions.includes('<Tooltip content="更多">')) {
  throw new Error("Agent session row actions should not show a visible 更多 tooltip bubble");
}

for (const token of ["const active = session.id === currentSessionId", "hover:bg-white/15", "hover:bg-accent"]) {
  if (!agentSessions.includes(token)) {
    throw new Error(`Agent session more button should style against active rows: missing ${token}`);
  }
}

for (const token of ["AgentSessionActionMenu", "DropdownMenu", "重命名", "置顶", "取消置顶", "删除"]) {
  if (!actionMenu.includes(token)) {
    throw new Error(`Agent session action menu should support rename/delete/pin: missing ${token}`);
  }
}

for (const forbidden of ["onMoveToGroup", "onNewGroup", "移入分组", "新建分组"]) {
  if (actionMenu.includes(forbidden)) {
    throw new Error(`Agent session menu should not expose normal group actions: found ${forbidden}`);
  }
}

for (const token of [
  "AgentSessions",
  "AgentSessionActionMenu",
  "agentMenuSession",
  "enterDraftWithAgent",
  "enterDraftWithAgent(agentId)",
  "onOpenAgent",
  "onOpenAgentList",
  "agentRefreshKey",
]) {
  if (!sessionManager.includes(token)) {
    throw new Error(`SessionManager should compose agent sessions with ${token}`);
  }
}

if (!normalSessions.includes('s.activeRoleKind !== "agent"')) {
  throw new Error("NormalSessions should exclude agent-owned sessions to avoid duplicate sidebar rows");
}

for (const token of ["onOpenAgent", "onOpenAgentList", "SessionManager"]) {
  if (!sidebar.includes(token)) {
    throw new Error(`Sidebar should pass agent actions to SessionManager: missing ${token}`);
  }
}

for (const token of [
  "handleOpenAgent",
  "handleOpenAgentList",
  'onNavigate({ section: "agents", agentId })',
  'agentId={location.section === "agents" ? location.agentId ?? null : null}',
]) {
  if (!appSource.includes(token)) {
    throw new Error(`App should route SessionManager agent actions into AgentsPage: missing ${token}`);
  }
}

for (const token of [
  '| { section: "agents"; agentId?: string | null }',
  'case "agents"',
  "left.agentId",
]) {
  if (!navigation.includes(token)) {
    throw new Error(`App navigation should preserve directed agent opening: missing ${token}`);
  }
}

for (const token of [
  "agentId",
  "useEffect",
  "setOpenId(agentId)",
]) {
  if (!agentsPage.includes(token)) {
    throw new Error(`AgentsPage should accept directed agent opening: missing ${token}`);
  }
}
