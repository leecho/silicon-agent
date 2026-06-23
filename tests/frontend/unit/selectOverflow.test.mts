import { readFileSync } from "node:fs";

const source = readFileSync("src/components/ui/Select.tsx", "utf8");

if (!source.includes("overflow-y-auto") || !source.includes("overflow-x-hidden")) {
  throw new Error("Select menu should allow vertical scrolling without horizontal scrollbars");
}

if (!source.includes('className="min-w-0 flex-1 overflow-hidden"')) {
  throw new Error("Select option content should be constrained to the menu width");
}
