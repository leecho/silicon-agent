import { existsSync, readFileSync } from "node:fs";

const modalPath = "src/components/layout/SessionSearchModal.tsx";
if (!existsSync(modalPath)) {
  throw new Error("Session search should have a dedicated modal component");
}

const modalSource = readFileSync(modalPath, "utf8");
const appSource = readFileSync("src/App.tsx", "utf8");
const sidebarSource = readFileSync("src/components/layout/Sidebar.tsx", "utf8");
const actionsSource = readFileSync("src/components/layout/SidebarTitlebarActions.tsx", "utf8");
const modalPrimitiveSource = readFileSync("src/components/ui/Modal.tsx", "utf8");

for (const required of [
  "listSessions",
  "isTopLevelSession",
  "!session.parentSessionId",
  "openSession",
  "openDraft",
  "autoFocus",
  "placeholder=\"搜索会话内容...\"",
  "所有任务",
  "共 {results.length} 个",
  "formatSessionAge",
  "index < 9",
  "⌘",
  "event.metaKey",
  'role="listbox"',
  'role="option"',
  'event.key === "ArrowDown"',
  'event.key === "ArrowUp"',
  'event.key === "Enter"',
  'padding="none"',
]) {
  if (!modalSource.includes(required)) {
    throw new Error(`SessionSearchModal should include ${required}`);
  }
}

if (!modalSource.includes("draftContent")) {
  throw new Error("Session search should match draft content as well as titles");
}

if (!appSource.includes("SessionSearchModal")) {
  throw new Error("App shell should render SessionSearchModal");
}
if (!appSource.includes("setSessionSearchOpen(true)")) {
  throw new Error("Collapsed sidebar search button should open the session search modal");
}
if (!sidebarSource.includes("onSearch") || !sidebarSource.includes("onSearch={onSearch}")) {
  throw new Error("Pinned sidebar should pass search action to titlebar actions");
}
if (!actionsSource.includes("onSearch")) {
  throw new Error("SidebarTitlebarActions should expose a search callback");
}
if (!modalPrimitiveSource.includes('padding = "default"')) {
  throw new Error("Modal should expose an explicit default padding mode");
}
if (!modalPrimitiveSource.includes('padding === "default" ? "p-5" : false')) {
  throw new Error("Modal should omit p-5 when callers request padding=\"none\"");
}
