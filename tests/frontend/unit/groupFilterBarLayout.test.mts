import { readFileSync } from "node:fs";

const source = readFileSync("src/components/groups/GroupFilterBar.tsx", "utf8");

for (const required of [
  "max-w-0",
  "overflow-hidden",
  "group-hover:max-w-[44px]",
  "group-focus-within:max-w-[44px]",
  "group-hover:ml-1",
  "transition-all duration-150",
]) {
  if (!source.includes(required)) {
    throw new Error(`GroupFilterBar group actions should collapse until hover: missing ${required}`);
  }
}

if (source.includes("group inline-flex items-center gap-1 rounded-full")) {
  throw new Error("Group chips should not reserve action-button spacing before hover");
}
