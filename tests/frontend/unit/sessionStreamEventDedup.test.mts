import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/session/SessionPage.tsx", "utf8");

for (const required of [
  "processedStreamEventKeysRef",
  "streamEventKey",
  "if (e.sequence > 0",
  "processedStreamEventKeysRef.current.has",
  "processedStreamEventKeysRef.current.add",
  "processedStreamEventKeysRef.current.clear",
]) {
  if (!source.includes(required)) {
    throw new Error(
      `SessionPage should ignore duplicate stream events by sequence: missing ${required}`,
    );
  }
}

const guardIndex = source.indexOf("processedStreamEventKeysRef.current.has");
const thinkingIndex = source.indexOf('if (e.kind === "thinking_delta")');
const messageIndex = source.indexOf('} else if (e.kind === "message_delta")');

if (guardIndex === -1 || thinkingIndex === -1 || messageIndex === -1) {
  throw new Error("SessionPage should contain stream event handling branches");
}

if (guardIndex > thinkingIndex || guardIndex > messageIndex) {
  throw new Error(
    "SessionPage should dedupe stream events before appending thinking or message deltas",
  );
}
