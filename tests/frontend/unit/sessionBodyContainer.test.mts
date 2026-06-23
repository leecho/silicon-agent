import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/session/SessionPage.tsx", "utf8");

if (
  !source.includes(
    'className="session-body flex min-h-0 min-w-0 flex-1 flex-col"',
  )
) {
  throw new Error(
    "Session feed, transient cards, and Composer should be grouped in a layout-neutral body container",
  );
}

const bodyIndex = source.indexOf(
  'className="session-body flex min-h-0 min-w-0 flex-1 flex-col"',
);
const feedIndex = source.indexOf("MessageFeed", bodyIndex);
const composerIndex = source.indexOf("<Composer", bodyIndex);

if (feedIndex === -1 || composerIndex === -1 || feedIndex > composerIndex) {
  throw new Error(
    "Session body container should contain MessageFeed before Composer",
  );
}

if (
  !source.includes("session-body") ||
  !source.includes("min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden")
) {
  throw new Error(
    "Session body scroll container should keep its existing flex and overflow behavior",
  );
}
