import { readFileSync } from "node:fs";

const cardSource = readFileSync("src/components/session/SessionPermissionCard.tsx", "utf8");
const narrativeSource = readFileSync("src/components/session/toolNarrative.ts", "utf8");

if (!narrativeSource.includes('create_agent: "创建智能体"')) {
  throw new Error("Tool display names should include create_agent as 创建智能体");
}

if (!narrativeSource.includes('create_team: "创建团队"')) {
  throw new Error("Tool display names should include create_team as 创建团队");
}

if (!cardSource.includes("toolDisplayName")) {
  throw new Error("SessionPermissionCard should render a display-name variable, not raw toolName");
}

if (cardSource.includes("pending.toolName}")) {
  throw new Error("SessionPermissionCard should not interpolate pending.toolName directly in UI text");
}
