import { readFileSync } from "node:fs";

const apiSource = readFileSync("src/api.ts", "utf8");
const messageFeedRowsSource = readFileSync("src/components/session/messageFeedRows.ts", "utf8");
const sessionPageSource = readFileSync("src/pages/session/SessionPage.tsx", "utf8");
const generalSource = readFileSync(
  "src/pages/settings/sections/GeneralConfigSection.tsx",
  "utf8",
);
const preferencesSource = readFileSync(
  "src/pages/settings/sections/PreferencesSection.tsx",
  "utf8",
);

for (const fn of ["getShowCompletedProcess", "setShowCompletedProcess"]) {
  if (!apiSource.includes(`function ${fn}`)) {
    throw new Error(`api.ts should expose ${fn}`);
  }
}

if (!apiSource.includes('"get_show_completed_process"')) {
  throw new Error("api.ts should bind get_show_completed_process");
}
if (!apiSource.includes('"set_show_completed_process"')) {
  throw new Error("api.ts should bind set_show_completed_process");
}

if (!preferencesSource.includes("显示已完成轮次的思考与执行过程")) {
  throw new Error("PreferencesSection should render completed process visibility setting");
}
if (generalSource.includes("显示已完成轮次的思考与执行过程")) {
  throw new Error("GeneralConfigSection should not render completed process visibility setting");
}
for (const symbol of [
  "getShowCompletedProcess",
  "setShowCompletedProcess",
  "showCompletedProcess",
  "toggleShowCompletedProcess",
]) {
  if (!preferencesSource.includes(symbol)) {
    throw new Error(`PreferencesSection should use ${symbol}`);
  }
  if (generalSource.includes(symbol)) {
    throw new Error(`GeneralConfigSection should not use ${symbol}`);
  }
}

if (!messageFeedRowsSource.includes("showProcess = true")) {
  throw new Error("buildPersistedRows should default to showing completed process details");
}
if (!messageFeedRowsSource.includes("reasoning: showProcess ? m.reasoning : undefined")) {
  throw new Error("buildPersistedRows should hide persisted assistant reasoning when disabled");
}
if (!messageFeedRowsSource.includes('} else if (showProcess && m.role === "tool")')) {
  throw new Error("buildPersistedRows should omit persisted tool rows when disabled");
}

if (!sessionPageSource.includes("showCompletedProcess")) {
  throw new Error("SessionPage should hold showCompletedProcess state");
}
if (!sessionPageSource.includes("getShowCompletedProcess")) {
  throw new Error("SessionPage should load showCompletedProcess from settings");
}
if (!sessionPageSource.includes("buildPersistedRows(messages, showCompletedProcess")) {
  throw new Error("SessionPage should pass showCompletedProcess only to persisted feed rebuild");
}

const runFinishedRebuildIndex = sessionPageSource.indexOf("rebuildFeed(d.messages", sessionPageSource.indexOf('e.kind === "run_finished"'));
if (runFinishedRebuildIndex < 0) {
  throw new Error("SessionPage should rebuild persisted feed after run_finished");
}
const runFinishedRebuildCall = sessionPageSource.slice(runFinishedRebuildIndex, runFinishedRebuildIndex + 80);
if (!runFinishedRebuildCall.includes("showCompletedProcess")) {
  throw new Error("SessionPage should preserve showCompletedProcess when run_finished rebuilds the feed");
}
