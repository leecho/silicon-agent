import { existsSync, readFileSync } from "node:fs";

const expectedFiles = [
  "src/components/layout/session-manager/sessionManagerShared.tsx",
  "src/components/layout/session-manager/useSessionManagerData.ts",
  "src/components/layout/session-manager/NormalSessions.tsx",
  "src/components/layout/session-manager/RemoteSessions.tsx",
  "src/components/layout/session-manager/SessionActionMenu.tsx",
  "src/components/layout/session-manager/GroupFormModal.tsx",
];

for (const path of expectedFiles) {
  if (!existsSync(path)) {
    throw new Error(`SessionManager split should create ${path}`);
  }
}

const sessionManagerSource = readFileSync("src/components/layout/SessionManager.tsx", "utf8");

if (existsSync("src/components/layout/session-manager/SessionManagerTemplate.tsx")) {
  throw new Error("SessionManager should not introduce a shared SessionManagerTemplate file");
}

for (const symbol of [
  "useSessionManagerData",
  "NormalSessions",
  "RemoteSessions",
  "SessionActionMenu",
  "GroupFormModal",
]) {
  if (!sessionManagerSource.includes(symbol)) {
    throw new Error(`SessionManager should compose ${symbol}`);
  }
}

for (const implementation of [
  "function draftTitle",
  "function remoteSessionNode",
  "function sessionNode",
  "function groupActions",
  "async function refreshAll",
  "subscribeSessionUpdated",
  "DropdownSubMenu",
  "ColorPicker",
]) {
  if (sessionManagerSource.includes(implementation)) {
    throw new Error(`SessionManager.tsx should not retain implementation detail: ${implementation}`);
  }
}
