import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/projects/ProjectHome.tsx", "utf8");

for (const token of [
  "selectedMemberId",
  "ProjectMemberDetailDrawer",
  "getMemberTasks",
  "runSessionId",
  "run.agentName === member.agentName",
  "task.assignee === member.agentName",
  "const displayName = member.displayName?.trim()",
  "task.assignee === displayName",
  "TaskStatusBadge",
  "负责的任务",
  "职责",
]) {
  if (!source.includes(token)) {
    throw new Error(`ProjectHome member detail drawer is missing ${token}`);
  }
}

if (source.includes("includes(member.agentName)") || source.includes("includes(label)")) {
  throw new Error("ProjectHome member task ownership should not use fragile substring matching");
}
