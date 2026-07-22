import { readFileSync } from "node:fs";

function mustInclude(file: string, needles: string[]) {
  const src = readFileSync(file, "utf8");
  for (const n of needles) {
    if (!src.includes(n)) {
      throw new Error(`${file} missing: ${n}`);
    }
  }
}

// composer 累计 chip
mustInclude("src/pages/session/composer/SessionUsageChip.tsx", ["累计", "UsageTotals", "formatTokens"]);
mustInclude("src/components/session/Composer.tsx", ["SessionUsageChip", "sessionUsage"]);
mustInclude("src/pages/session/SessionPage.tsx", ["getSessionUsage", "sessionUsage", "setSessionUsage"]);

// 项目用量卡片 + 详情
mustInclude("src/pages/projects/ProjectHome.tsx", ["onGoUsage", "用量"]);
mustInclude("src/pages/projects/ProjectView.tsx", ["ProjectUsage", "getProjectUsage", '"usage"']);

// 设置页两个 tab
mustInclude("src/pages/settings/sections/UsageAnalysisSection.tsx", ["UsageProjectsPanel", "UsageAgentsPanel", '"projects"', '"agents"']);
mustInclude("src/pages/settings/sections/usage/UsageProjectsPanel.tsx", ["byProject", "项目"]);
mustInclude("src/pages/settings/sections/usage/UsageAgentsPanel.tsx", ["byAgent", "智能体"]);

// api 封装
mustInclude("src/api.ts", ["get_session_usage", "get_project_usage", "get_agent_usage"]);

console.log("token usage UI assertions passed");
