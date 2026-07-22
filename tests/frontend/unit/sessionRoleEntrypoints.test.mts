import { readFileSync } from "node:fs";

const providerSource = readFileSync("src/components/session/SessionProvider.tsx", "utf8");
const draftSource = readFileSync("src/pages/session/SessionDraftPage.tsx", "utf8");
const popularExpertsSource = readFileSync("src/pages/session/PopularExpertsBar.tsx", "utf8");
const expertDetailSource = readFileSync("src/pages/experts/ExpertDetailDrawer.tsx", "utf8");
const expertPlazaSource = readFileSync("src/pages/experts/ExpertPlaza.tsx", "utf8");
const expertsPageSource = readFileSync("src/pages/experts/ExpertsPage.tsx", "utf8");
const agentViewSource = readFileSync("src/pages/agents/AgentView.tsx", "utf8");
const sessionManagerSource = readFileSync("src/components/layout/SessionManager.tsx", "utf8");
const teamPagesSource = [
  readFileSync("src/pages/teams/TeamDetailDrawer.tsx", "utf8"),
  readFileSync("src/pages/teams/TeamPlaza.tsx", "utf8"),
  readFileSync("src/pages/teams/TeamsPage.tsx", "utf8"),
].join("\n");
const typesSource = readFileSync("src/types.ts", "utf8");

for (const required of [
  "enterDraftWithExpert: (expertId: string, content?: string) => void",
  "enterDraftWithAgent: (agentId: string, content?: string) => void",
  "enterDraftWithTeam: (teamId: string, content?: string) => void",
  "enterDraftWithProject: (projectId: string, content?: string) => void",
  "setDraftSeedRole({ kind: \"expert\", id: expertId })",
  "setDraftSeedAgentId(agentId)",
  "setDraftSeedRole({ kind: \"team\", id: teamId })",
]) {
  if (!providerSource.includes(required)) {
    throw new Error(`SessionProvider should expose typed role draft entrypoints: missing ${required}`);
  }
}

if (providerSource.includes("setDraftSeedRole({ kind: \"agent\", id: agentId })")) {
  throw new Error("SessionProvider should seed persistent agents through draftSeedAgentId, not the role slot.");
}

if (providerSource.includes("enterDraftWithRole:") || providerSource.includes("const enterDraftWithRole")) {
  throw new Error("SessionProvider should not expose the generic enterDraftWithRole API.");
}

for (const source of [expertDetailSource, expertPlazaSource, expertsPageSource]) {
  if (!source.includes("enterDraftWithExpert")) {
    throw new Error("Expert entry pages should use enterDraftWithExpert.");
  }
  for (const forbidden of [
    'enterDraftWithRole("expert"',
    "enterDraftWithRole",
    "onUse(agent.name)",
  ]) {
    if (source.includes(forbidden)) {
      throw new Error(`Expert entry pages should pass expert ids, not names: found ${forbidden}`);
    }
  }
}

for (const required of [
  "onPickExpert={(expertId) => void pickRole(\"expert\", expertId)}",
]) {
  if (!draftSource.includes(required)) {
    throw new Error(`SessionDraftPage should set expert roles by id: missing ${required}`);
  }
}

for (const required of [
  "roleId === agent.id",
  "seedRole.id === agent.id",
  "onPickExpert(agent.id)",
]) {
  if (!popularExpertsSource.includes(required)) {
    throw new Error(`PopularExpertsBar should compare and emit expert ids: missing ${required}`);
  }
}

for (const forbidden of [
  "roleId === agent.name",
  "seedRole.id === agent.name",
  "onPickExpert(agent.name)",
]) {
  if (popularExpertsSource.includes(forbidden)) {
    throw new Error(`PopularExpertsBar should not use expert names as role ids: found ${forbidden}`);
  }
}

if (!agentViewSource.includes("enterDraftWithAgent(agent.id)")) {
  throw new Error("AgentView should enter agent drafts through enterDraftWithAgent.");
}

if (!sessionManagerSource.includes("enterDraftWithAgent(agentId)")) {
  throw new Error("SessionManager should enter agent drafts through enterDraftWithAgent.");
}

if (!teamPagesSource.includes("enterDraftWithTeam")) {
  throw new Error("Team pages should enter team drafts through enterDraftWithTeam.");
}

if (!typesSource.includes('roleKind?: "expert" | "team" | null;') || !typesSource.includes("agentId?: string | null;")) {
  throw new Error("SessionInfo should keep expert/team roles separate from persistent agentId.");
}
