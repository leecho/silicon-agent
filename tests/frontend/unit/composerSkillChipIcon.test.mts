import { readFileSync } from "node:fs";

const source = readFileSync("src/components/session/ComposerInput.tsx", "utf8");

if (!source.includes("chipIcon")) {
  throw new Error("Composer skill chip should create a dedicated icon node");
}

if (!source.includes('kind === "skill"')) {
  throw new Error("Composer chip icon should be scoped to skill chips");
}

if (!source.includes('iconEl.ariaHidden = "true"')) {
  throw new Error("Composer skill chip icon should be hidden from assistive text");
}

if (!source.includes("chip.append(iconEl, labelEl, remove)")) {
  throw new Error("Composer skill chip should render icon before label and remove control");
}

const makeChipCallCount = source.match(/makeChip\("skill"/g)?.length ?? 0;
if (makeChipCallCount < 3) {
  throw new Error("All composer skill insertion paths should keep using makeChip");
}
