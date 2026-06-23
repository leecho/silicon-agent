import { existsSync, readFileSync } from "node:fs";

const actionsPath = "src/components/layout/SidebarTitlebarActions.tsx";
if (!existsSync(actionsPath)) {
  throw new Error("Sidebar titlebar actions should be shared between pinned and collapsed sidebar states");
}

const actionsSource = readFileSync(actionsPath, "utf8");
const sidebarSource = readFileSync("src/components/layout/Sidebar.tsx", "utf8");
const appSource = readFileSync("src/App.tsx", "utf8");

if (!actionsSource.includes("Search") || !actionsSource.includes('aria-label="搜索"')) {
  throw new Error("Shared sidebar titlebar actions should include the search button");
}

if (actionsSource.includes("useSession")) {
  throw new Error("Shared sidebar titlebar actions should receive session actions via props");
}

if (!actionsSource.includes("onNewTask")) {
  throw new Error("Shared sidebar titlebar actions should expose a new task callback prop");
}

if (!sidebarSource.includes("<SidebarTitlebarActions")) {
  throw new Error("Pinned sidebar should render the shared titlebar actions");
}

if (!appSource.includes("<SidebarTitlebarActions")) {
  throw new Error("Collapsed sidebar should render the shared titlebar actions");
}

if (!appSource.includes("getAppPlatform") || !appSource.includes("--titlebar-collapsed-actions-left")) {
  throw new Error("App shell should derive collapsed titlebar spacing from the runtime platform");
}

if (!appSource.includes("silicon-agent.dev.appPlatform") || !appSource.includes("sw-platform")) {
  throw new Error("App shell should expose a dev-only platform override for titlebar simulation");
}

if (!appSource.includes("function isDevBuild") || !appSource.includes(".env?.DEV")) {
  throw new Error("Platform override should be gated to development builds");
}
