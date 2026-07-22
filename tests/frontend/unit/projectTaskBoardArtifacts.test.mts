import { readFileSync } from "node:fs";

const boardSource = readFileSync("src/pages/projects/ProjectTaskBoard.tsx", "utf8");
const viewSource = readFileSync("src/pages/projects/ProjectView.tsx", "utf8");

for (const token of [
  "artifactCountByTaskId",
  "onOpenArtifacts",
  "TaskArtifactCountButton",
  "产物",
]) {
  if (!boardSource.includes(token)) {
    throw new Error(`ProjectTaskBoard should expose per-task artifact stats: missing ${token}`);
  }
}

if (!boardSource.includes("task.runSessionId || task.threadSessionId")) {
  throw new Error("ProjectTaskBoard should treat self-handled tasks as openable through their project session");
}

for (const token of [
  "listProjectArtifacts",
  "ProjectArtifact",
  "artifactCountByTaskId",
  "artifact.task",
  "setView(\"artifacts\")",
]) {
  if (!viewSource.includes(token)) {
    throw new Error(`ProjectView should aggregate artifacts for the task board and navigate to artifacts: missing ${token}`);
  }
}
