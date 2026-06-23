import { readFileSync } from "node:fs";

const composerSource = readFileSync("src/components/session/Composer.tsx", "utf8");
const stylesSource = readFileSync("src/styles.css", "utf8");

const runActivityStart = composerSource.indexOf("function RunActivityInline");
const composerStart = composerSource.indexOf("export function Composer", runActivityStart);
const runActivitySource =
  runActivityStart >= 0 && composerStart > runActivityStart
    ? composerSource.slice(runActivityStart, composerStart)
    : "";

if (!runActivitySource.includes("ui-activity-breathe")) {
  throw new Error("RunActivityInline should own the breathing background");
}

if (composerSource.includes('running ? "composer-running-breathe')) {
  throw new Error("Composer input container should not own the run breathing background");
}

for (const required of [
  "@keyframes ui-activity-breathe-pulse",
  ".ui-activity-breathe::before",
  "animation: ui-activity-breathe-pulse",
  "pointer-events: none",
]) {
  if (!stylesSource.includes(required)) {
    throw new Error(`styles.css should define neutral run activity breathing effect: missing ${required}`);
  }
}
