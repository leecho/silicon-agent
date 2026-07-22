import { existsSync, readFileSync } from "node:fs";

const scheduledTaskSessionsPath = "src/components/scheduling/ScheduledTaskSessions.tsx";
const legacyTaskTreePath = "src/components/scheduling/TaskTree.tsx";

if (!existsSync(scheduledTaskSessionsPath)) {
  throw new Error("Scheduled task sessions should live in ScheduledTaskSessions.tsx");
}

if (existsSync(legacyTaskTreePath)) {
  throw new Error("Legacy TaskTree.tsx should be renamed to ScheduledTaskSessions.tsx");
}

const scheduledTaskSessionsSource = readFileSync(scheduledTaskSessionsPath, "utf8");
const sessionManagerSource = readFileSync("src/components/layout/SessionManager.tsx", "utf8");
const typesSource = readFileSync("src/types.ts", "utf8");

if (!scheduledTaskSessionsSource.includes("export function ScheduledTaskSessions()")) {
  throw new Error("ScheduledTaskSessions.tsx should export ScheduledTaskSessions");
}

if (!sessionManagerSource.includes('import { ScheduledTaskSessions } from "../scheduling/ScheduledTaskSessions";')) {
  throw new Error("SessionManager should import ScheduledTaskSessions");
}

if (!sessionManagerSource.includes("<ScheduledTaskSessions />")) {
  throw new Error("SessionManager should render ScheduledTaskSessions");
}

if (sessionManagerSource.includes("TaskTree") || typesSource.includes("TaskTree")) {
  throw new Error("TaskTree name should not remain in SessionManager or task execution comments");
}
