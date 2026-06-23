import { readFileSync } from "node:fs";

const sessionPageSource = readFileSync("src/pages/session/SessionPage.tsx", "utf8");
const monitorPanelSource = readFileSync("src/components/session/SessionMonitorPanel.tsx", "utf8");
const workspacePanelSource = readFileSync("src/components/session/SessionArtifact.tsx", "utf8");
const apiSource = readFileSync("src/api.ts", "utf8");
const commandsSource = readFileSync("src-tauri/src/commands/mod.rs", "utf8");
const libSource = readFileSync("src-tauri/src/lib.rs", "utf8");

if (!workspacePanelSource.includes("event.stopPropagation()")) {
  throw new Error("Workspace open button should stop propagation before opening the directory");
}

if (!sessionPageSource.includes("const handleOpenWorkspace = async () =>")) {
  throw new Error("SessionPage should own an explicit handleOpenWorkspace action");
}

if (!apiSource.includes('invoke<void>("open_session_workspace", { sessionId })')) {
  throw new Error("API should open a workspace through the backend session-scoped command");
}

if (!sessionPageSource.includes("await openSessionWorkspace(detail.session.id)")) {
  throw new Error("handleOpenWorkspace should call openSessionWorkspace with the current session id");
}

if (sessionPageSource.includes("await openWorkspaceDir(detail.resolvedWorkingDir)")) {
  throw new Error("SessionPage should not send arbitrary local paths to the frontend opener API");
}

if (!commandsSource.includes("pub fn open_session_workspace(")) {
  throw new Error("Backend should expose an open_session_workspace command");
}

if (!commandsSource.includes(".get_session(&session_id)?") || !commandsSource.includes(".ok_or_else(|| \"session not found\".to_string())?")) {
  throw new Error("open_session_workspace should verify the session exists before resolving a workspace");
}

if (!commandsSource.includes("services.ensure_session_workspace(&session_id)?")) {
  throw new Error("open_session_workspace should resolve and create the session workspace server-side");
}

if (!commandsSource.includes("app.opener()") || !commandsSource.includes(".open_path(")) {
  throw new Error("open_session_workspace should open the resolved workspace through the Rust opener extension");
}

if (!libSource.includes("commands::open_session_workspace")) {
  throw new Error("open_session_workspace should be registered in the Tauri invoke handler");
}

if (!sessionPageSource.includes('notify.error("打开工作目录失败：" + String(err))')) {
  throw new Error("Opening the workspace should surface failures to the user");
}

if (!sessionPageSource.includes("onOpenWorkspace={handleOpenWorkspace}")) {
  throw new Error("SessionMonitorPanel should receive the workspace open action directly");
}

if (!monitorPanelSource.includes("onOpen={onOpenWorkspace}")) {
  throw new Error("SessionMonitorPanel should pass the workspace open action to WorkspacePanel");
}
