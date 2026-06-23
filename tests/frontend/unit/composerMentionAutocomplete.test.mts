import { readFileSync } from "node:fs";

const composerSource = readFileSync("src/components/session/Composer.tsx", "utf8");
const inputSource = readFileSync("src/components/session/ComposerInput.tsx", "utf8");
const apiSource = readFileSync("src/api.ts", "utf8");
const artifactCommandsSource = readFileSync("src-tauri/src/commands/artifact.rs", "utf8");
const libSource = readFileSync("src-tauri/src/lib.rs", "utf8");

if (!apiSource.includes("export async function listSessionWorkspaceFiles")) {
  throw new Error("API should expose listSessionWorkspaceFiles for composer @ mentions");
}

if (!apiSource.includes('invoke<string[]>("list_session_workspace_files", { sessionId })')) {
  throw new Error("Workspace file suggestions should be loaded through a session-scoped backend command");
}

if (!libSource.includes("commands::list_session_workspace_files")) {
  throw new Error("list_session_workspace_files should be registered in the Tauri invoke handler");
}

if (!artifactCommandsSource.includes("pub fn list_session_workspace_files(")) {
  throw new Error("Backend should expose a command for session workspace file autocomplete");
}

if (!artifactCommandsSource.includes("ensure_session_workspace(&session_id)?")) {
  throw new Error("Workspace file autocomplete should resolve the workspace server-side by session id");
}

if (!composerSource.includes("listSessionWorkspaceFiles")) {
  throw new Error("Composer should load workspace files for @ mention suggestions");
}

if (!composerSource.includes("workspaceFiles={workspaceFiles}")) {
  throw new Error("Composer should pass workspace files into ComposerInput");
}

if (!inputSource.includes("type MentionTrigger = \"/\" | \"@\"")) {
  throw new Error("ComposerInput should model slash and at-sign mention triggers explicitly");
}

if (!inputSource.includes("detectMention")) {
  throw new Error("ComposerInput should use a generic mention detector instead of slash-only detection");
}

if (!inputSource.includes("([^\\s@]*)")) {
  throw new Error("@ file mention query should allow slash-delimited workspace paths");
}

if (!inputSource.includes("workspaceFiles: string[]")) {
  throw new Error("ComposerInput should accept workspace file suggestion data");
}

if (!inputSource.includes("type: \"workspaceFile\"")) {
  throw new Error("@ mention menu should include workspace file items");
}

if (!inputSource.includes("makeChip(\"file\", item.path, item.path)")) {
  throw new Error("Selecting a workspace file should insert a serializable @file chip");
}
