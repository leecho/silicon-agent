import { readFileSync } from "node:fs";

const messageFeedSource = readFileSync("src/components/session/MessageFeed.tsx", "utf8");
const rowsSource = readFileSync("src/components/session/messageFeedRows.ts", "utf8");
const toolStepsSource = readFileSync("src/components/session/MessageFeedToolSteps.tsx", "utf8");
const sessionPageSource = readFileSync("src/pages/session/SessionPage.tsx", "utf8");
const typesSource = readFileSync("src/types.ts", "utf8");

for (const field of ["startedAt?: number", "finishedAt?: number"]) {
  if (!typesSource.includes(field)) {
    throw new Error(`Tool feed rows should expose ${field}`);
  }
}

for (const required of [
  "startedAtByCallId",
  "parseMessageTimestamp(m.createdAt)",
  "finishedAt: parseMessageTimestamp(m.createdAt)",
]) {
  if (!rowsSource.includes(required)) {
    throw new Error(
      `buildPersistedRows should reconstruct persisted tool step duration: missing ${required}`,
    );
  }
}

for (const required of ["formatStepElapsed", "StepDuration"]) {
  if (!toolStepsSource.includes(required)) {
    throw new Error(
      `MessageFeed should render per-step elapsed duration: missing ${required}`,
    );
  }
}
if (!messageFeedSource.includes("hasActiveStep")) {
  throw new Error("MessageFeed should maintain active-step timer state");
}

const formatStart = toolStepsSource.indexOf("function formatStepElapsed");
const formatEnd = toolStepsSource.indexOf("function StepDuration", formatStart);
const formatSource =
  formatStart >= 0 && formatEnd > formatStart
    ? toolStepsSource.slice(formatStart, formatEnd)
    : "";

for (const required of ['"<1s"', "`${totalSeconds}s`", "`${minutes}m ", "`${hours}h "]) {
  if (!formatSource.includes(required)) {
    throw new Error(`Step duration should use compact h/m/s units: missing ${required}`);
  }
}

for (const legacyUnit of ["秒", "分", "小时"]) {
  if (formatSource.includes(legacyUnit)) {
    throw new Error(`Step duration should not use Chinese duration unit ${legacyUnit}`);
  }
}

for (const required of [
  "startedAt: parseEpochSeconds(e.createdAt) ?? Date.now()",
  "r.finishedAt = parseEpochSeconds(e.createdAt) ?? Date.now()",
  "r.startedAt = r.startedAt ?? r.finishedAt",
]) {
  if (!sessionPageSource.includes(required)) {
    throw new Error(
      `SessionPage should maintain live tool step duration: missing ${required}`,
    );
  }
}
