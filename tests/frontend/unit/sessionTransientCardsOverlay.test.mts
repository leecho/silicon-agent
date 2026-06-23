import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/session/SessionPage.tsx", "utf8");

const bodyIndex = source.indexOf(
  'className="session-body flex min-h-0 min-w-0 flex-1 flex-col"',
);
if (bodyIndex === -1) {
  throw new Error("SessionPage should keep a session-body layout container");
}

const scrollIndex = source.indexOf("ref={feedScrollRef}", bodyIndex);
const stackIndex = source.indexOf('className="session-transient-stack', bodyIndex);
if (scrollIndex === -1 || stackIndex === -1 || scrollIndex > stackIndex) {
  throw new Error("Session transient stack should render after the history scroll region");
}

const composerIndex = source.indexOf("<Composer", bodyIndex);
if (stackIndex === -1 || composerIndex === -1 || stackIndex > composerIndex) {
  throw new Error(
    "Session transient cards should render in a stack immediately above Composer",
  );
}

const stackRegion = source.slice(stackIndex, composerIndex);
for (const cardName of ["SessionPermissionCard", "SessionAskCard"]) {
  if (!stackRegion.includes(`<${cardName}`)) {
    throw new Error(`${cardName} should render inside the transient stack`);
  }
}

for (const required of [
  "SessionPermissionCard",
  "SessionAskCard",
  "pointer-events-none",
  "pointer-events-auto",
]) {
  if (!stackRegion.includes(required)) {
    throw new Error(`Session transient stack should include ${required}`);
  }
}
