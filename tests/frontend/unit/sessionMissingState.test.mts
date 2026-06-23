import { readFileSync } from "node:fs";

const apiSource = readFileSync("src/api.ts", "utf8");
const pageSource = readFileSync("src/pages/session/SessionPage.tsx", "utf8");

if (!apiSource.includes("Promise<Session | null>")) {
  throw new Error("getSession should type the missing-session case as Session | null");
}

if (!pageSource.includes('"loading"') || !pageSource.includes('"missing"')) {
  throw new Error("SessionPage should use explicit loading and missing states");
}

if (!pageSource.includes("if (d === null)")) {
  throw new Error("SessionPage should handle getSession null before reading messages");
}

if (!pageSource.includes("会话不存在")) {
  throw new Error("Missing sessions should render a dedicated empty state");
}

const nullGuardIndex = pageSource.indexOf("if (d === null)");
const rebuildIndex = pageSource.search(/rebuildFeed\(d\.messages(?:,|\))/);

if (nullGuardIndex === -1 || rebuildIndex === -1 || nullGuardIndex > rebuildIndex) {
  throw new Error("Missing-session guard should run before rebuilding the feed");
}

if (
  !pageSource.includes("requestNewSession()") ||
  !pageSource.includes("refreshSessions()")
) {
  throw new Error("Missing-session UI should offer recovery actions");
}
