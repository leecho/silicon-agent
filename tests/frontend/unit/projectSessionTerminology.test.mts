import { readFileSync } from "node:fs";

const files = [
  "src/api.ts",
  "src/types.ts",
  "src/pages/home/HomePage.tsx",
  "src/pages/projects/ProjectArtifactList.tsx",
  "src/pages/projects/ProjectView.tsx",
  "src/pages/projects/ProjectHome.tsx",
  "src/components/layout/session-manager/ProjectSessions.tsx",
  "src/components/session/SessionTaskLedger.tsx",
  "src/components/session/SessionMonitorPanel.tsx",
  "tests/frontend/unit/projectTaskBoardArtifacts.test.mts",
];

const bannedTerm = "\u7ebf\u7a0b";

for (const file of files) {
  const source = readFileSync(file, "utf8");
  if (source.includes(bannedTerm)) {
    throw new Error(`${file} should call project conversation concepts 会话, not thread terminology`);
  }
}

const uiFiles = files.filter((file) => file !== "src/api.ts" && file !== "src/types.ts");
for (const file of uiFiles) {
  const source = readFileSync(file, "utf8");
  for (const legacy of ["listProjectThreads", "threadCount", "ProjectThread", "onOpenThread", "onNewThread"]) {
    if (source.includes(legacy)) {
      throw new Error(`${file} should use project session terminology, not ${legacy}`);
    }
  }
}
