import { existsSync, readFileSync } from "node:fs";

const tabsSource = readFileSync("src/pages/settings/settingsTabs.ts", "utf8");
const pageSource = readFileSync("src/pages/settings/SettingsPage.tsx", "utf8");
const preferencesSource = readFileSync(
  "src/pages/settings/sections/PreferencesSection.tsx",
  "utf8",
);
const providerSource = readFileSync(
  "src/pages/settings/sections/ProviderSection.tsx",
  "utf8",
);
const generalPath = "src/pages/settings/sections/GeneralConfigSection.tsx";

if (!tabsSource.includes('"model-advance"')) {
  throw new Error("Settings tabs should include a model-advance tab");
}

const generalTabStart = tabsSource.indexOf('id: "model-advance"');
const providerTabStart = tabsSource.indexOf('id: "model-provider"');
if (
  generalTabStart === -1 ||
  providerTabStart === -1 ||
  generalTabStart > providerTabStart
) {
  throw new Error(
    "General config should appear before model provider config in the model group",
  );
}

const generalTabSource = tabsSource.slice(
  Math.max(0, generalTabStart - 120),
  generalTabStart + 260,
);
if (!generalTabSource.includes('label: "通用配置"')) {
  throw new Error("General config tab should be labeled 通用配置");
}
if (!generalTabSource.includes('group: "模型"')) {
  throw new Error("General config tab should live in the 模型 group");
}

if (!existsSync(generalPath)) {
  throw new Error("GeneralConfigSection should exist");
}

if (!pageSource.includes("GeneralConfigSection")) {
  throw new Error("SettingsPage should render GeneralConfigSection");
}

const generalSource = readFileSync(generalPath, "utf8");
for (const label of [
  "快捷建议",
  "自动压缩上下文",
  "上下文压缩阈值",
  "最大迭代次数",
  "失败自动重试次数",
  "备用模型（fallback）",
  "辅助模型",
]) {
  if (!generalSource.includes(label)) {
    throw new Error(`General config should include ${label}`);
  }
  if (preferencesSource.includes(label)) {
    throw new Error(`PreferencesSection should no longer include ${label}`);
  }
}

const autoCompactIndex = generalSource.indexOf('title="自动压缩上下文"');
const compactThresholdIndex = generalSource.indexOf('title="上下文压缩阈值"');
if (autoCompactIndex === -1 || compactThresholdIndex === -1) {
  throw new Error(
    "General config should include auto compact and compact threshold settings",
  );
}
if (compactThresholdIndex < autoCompactIndex) {
  throw new Error(
    "Context compact threshold should be configured after auto compact switch",
  );
}

for (const apiName of [
  "getMaxIterations",
  "setMaxIterations",
  "getAutoCompactThresholdPct",
  "setAutoCompactThresholdPct",
]) {
  if (!generalSource.includes(apiName)) {
    throw new Error(`General config should use ${apiName}`);
  }
}

for (const label of ["备用模型（fallback）", "辅助模型"]) {
  if (providerSource.includes(label)) {
    throw new Error(`ProviderSection should no longer include ${label}`);
  }
}
