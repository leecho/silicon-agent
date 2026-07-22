import { readFileSync } from "node:fs";

const source = readFileSync("src/components/session-tree/SessionTree.tsx", "utf8");

const requiredPatterns = [
  /function groupHeaderPadding/,
  /const isGroup = node\.children !== undefined/,
  /isGroup\s*\?\s*"text-foreground-muted hover:text-foreground"/,
  /font-semibold uppercase/,
  /isGroup\s*\?\s*"pr-2\.5 pb-1 pt-2"/,
  /isGroup\s*\?\s*groupHeaderPadding\(depth\)\s*:\s*nodePadding\(depth\)/,
];

for (const pattern of requiredPatterns) {
  if (!pattern.test(source)) {
    throw new Error(`SessionTree group rows should keep first-level group styling: missing ${pattern}`);
  }
}
