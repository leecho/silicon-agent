import { readFileSync } from "node:fs";

function assertIncludes(source: string, pattern: string, message: string) {
  if (!source.includes(pattern)) {
    throw new Error(message);
  }
}

function assertNotIncludes(source: string, pattern: string, message: string) {
  if (source.includes(pattern)) {
    throw new Error(message);
  }
}

const settingsPage = readFileSync("src/pages/settings/SettingsPage.tsx", "utf8");
const appShell = readFileSync("src/App.tsx", "utf8");

assertNotIncludes(
  settingsPage,
  "ArrowRight",
  "settings page should not render a forward navigation button",
);

assertNotIncludes(
  settingsPage,
  "canForward",
  "settings page should not accept forward-history state",
);

assertNotIncludes(
  settingsPage,
  "canBack",
  "settings page should not accept back-history state",
);

assertNotIncludes(
  settingsPage,
  "onForward",
  "settings page should not accept a forward-history handler",
);

assertIncludes(
  appShell,
  "onSelectTab={(tab) => navigation.replace({ section: \"settings\", tab })}",
  "settings tab changes should replace the current settings location instead of pushing history",
);

assertIncludes(
  appShell,
  "function handleSettingsBack() {\n    navigation.replace({ section: \"session\" });",
  "settings return should go directly to the app session area instead of consuming back history",
);

assertNotIncludes(
  appShell,
  "navigation.back();",
  "settings return should not call history back",
);
