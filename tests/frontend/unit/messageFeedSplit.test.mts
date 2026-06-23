import { existsSync, readFileSync } from "node:fs";

const feedPath = "src/components/session/MessageFeed.tsx";
const rowsPath = "src/components/session/messageFeedRows.ts";
const toolStepsPath = "src/components/session/MessageFeedToolSteps.tsx";
const assistantAnswerPath = "src/components/session/AssistantAnswer.tsx";
const artifactsPath = "src/components/session/RoundArtifacts.tsx";

for (const path of [rowsPath, toolStepsPath, assistantAnswerPath, artifactsPath]) {
  if (!existsSync(path)) {
    throw new Error(`MessageFeed split should create ${path}`);
  }
}

const feedSource = readFileSync(feedPath, "utf8");

for (const symbol of ["buildPersistedRows", "groupRows", "TraceStepGroup", "AssistantAnswer", "RoundArtifacts"]) {
  if (!feedSource.includes(symbol)) {
    throw new Error(`MessageFeed should import and use ${symbol}`);
  }
}

for (const implementation of [
  "function buildPersistedRows",
  "function formatStepElapsed",
  "function AssistantAnswer",
  "function RoundArtifacts",
]) {
  if (feedSource.includes(implementation)) {
    throw new Error(`MessageFeed.tsx should not retain implementation ${implementation}`);
  }
}
