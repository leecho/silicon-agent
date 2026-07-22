import { readFileSync } from "node:fs";

const source = readFileSync("src/components/scheduling/ScopedScheduledTasks.tsx", "utf8");

for (const token of [
  'className="rounded-xl border border-border-subtle bg-surface p-4"',
  "任务数量",
  "stats.tasks",
]) {
  if (!source.includes(token)) {
    throw new Error(`ScopedScheduledTasks should render compact single-column task count card: missing ${token}`);
  }
}

for (const removed of [
  "lg:col-span-2",
  "执行次数",
  "成功次数",
  "失败次数",
  "grid grid-cols-4",
  "stats.executions",
  "stats.succeeded",
  "stats.failed",
]) {
  if (source.includes(removed)) {
    throw new Error(`ScopedScheduledTasks should not render expanded execution metrics: ${removed}`);
  }
}
