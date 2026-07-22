import { readFileSync } from "node:fs";

const provider = readFileSync("src/components/session/SessionProvider.tsx", "utf8");
const draftPage = readFileSync("src/pages/session/SessionDraftPage.tsx", "utf8");
const workspacePicker = readFileSync("src/pages/session/composer/WorkspacePicker.tsx", "utf8");
const projectSessions = readFileSync("src/components/layout/session-manager/ProjectSessions.tsx", "utf8");
const projectView = readFileSync("src/pages/projects/ProjectView.tsx", "utf8");
const api = readFileSync("src/api.ts", "utf8");

for (const token of [
  "draftSeedProjectId",
  "enterDraftWithProject",
  "setDraftSeedProjectId(projectId)",
]) {
  if (!provider.includes(token)) {
    throw new Error(`SessionProvider should support project draft context: missing ${token}`);
  }
}

for (const token of [
  "listProjects",
  "submitProjectDraftMessage",
  "draftProjectId",
  "setDraftProjectId",
  "openSession(projectSessionId)",
]) {
  if (!draftPage.includes(token)) {
    throw new Error(`SessionDraftPage should submit selected project drafts at send time: missing ${token}`);
  }
}

for (const token of [
  "projects?: Project[]",
  "onPickProject?:",
  "项目",
  "本地目录",
  "暂无项目",
]) {
  if (!workspacePicker.includes(token)) {
    throw new Error(`WorkspacePicker should expose project and directory context groups: missing ${token}`);
  }
}

for (const source of [projectSessions, projectView, api]) {
  if (source.includes("createProjectThread")) {
    throw new Error("Frontend should not call createProjectThread; project sessions start from drafts");
  }
}

if (!api.includes("submitProjectDraftMessage") || !api.includes('"submit_project_draft_message"')) {
  throw new Error("API binding should expose submitProjectDraftMessage");
}
