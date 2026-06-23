import { readFileSync } from "node:fs";

const apiSource = readFileSync("src/api.ts", "utf8");
const sessionPageSource = readFileSync("src/pages/session/SessionPage.tsx", "utf8");
const generalSource = readFileSync(
  "src/pages/settings/sections/GeneralConfigSection.tsx",
  "utf8",
);
const preferencesSource = readFileSync(
  "src/pages/settings/sections/PreferencesSection.tsx",
  "utf8",
);
const runtimeCommandsSource = readFileSync("src-tauri/src/commands/runtime.rs", "utf8");
const appSettingsSource = readFileSync("src-tauri/src/app_settings/mod.rs", "utf8");
const libSource = readFileSync("src-tauri/src/lib.rs", "utf8");

for (const fn of [
  "getSessionTaskPanelDefaultVisible",
  "setSessionTaskPanelDefaultVisible",
]) {
  if (!apiSource.includes(`function ${fn}`)) {
    throw new Error(`api.ts should expose ${fn}`);
  }
}

for (const command of [
  "get_session_task_panel_default_visible",
  "set_session_task_panel_default_visible",
]) {
  if (!apiSource.includes(`"${command}"`)) {
    throw new Error(`api.ts should bind ${command}`);
  }
  if (!runtimeCommandsSource.includes(`fn ${command}`)) {
    throw new Error(`runtime commands should expose ${command}`);
  }
  if (!libSource.includes(`commands::${command}`)) {
    throw new Error(`lib.rs should register ${command}`);
  }
}

if (!appSettingsSource.includes("session_task_panel_default_visible")) {
  throw new Error("AppSettingsStore should persist session task panel default visibility");
}

for (const symbol of [
  "getSessionTaskPanelDefaultVisible",
  "setSessionTaskPanelDefaultVisible",
  "sessionTaskPanelDefaultVisible",
  "toggleSessionTaskPanelDefaultVisible",
]) {
  if (!preferencesSource.includes(symbol)) {
    throw new Error(`PreferencesSection should use ${symbol}`);
  }
  if (generalSource.includes(symbol)) {
    throw new Error(`GeneralConfigSection should not use ${symbol}`);
  }
}

if (!preferencesSource.includes("默认显示任务面板")) {
  throw new Error("PreferencesSection should render the task panel default setting");
}
if (generalSource.includes("默认显示任务面板")) {
  throw new Error("GeneralConfigSection should not render the task panel default setting");
}

if (!sessionPageSource.includes("getSessionTaskPanelDefaultVisible")) {
  throw new Error("SessionPage should load the task panel default visibility setting");
}
if (!sessionPageSource.includes("setCollapsedMonitorSessionId(showTaskPanelDefault ? null : sessionId)")) {
  throw new Error("SessionPage should initialize task panel visibility from the setting");
}
if (!sessionPageSource.includes('aria-label="展开任务面板"')) {
  throw new Error("SessionPage should expose a manual task panel expand button when collapsed");
}
if (!sessionPageSource.includes("onCollapse={() => setCollapsedMonitorSessionId(detail.session.id)}")) {
  throw new Error("SessionMonitorPanel should handle manual task panel collapse while expanded");
}
