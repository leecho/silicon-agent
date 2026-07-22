import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/settings/sections/MemorySection.tsx", "utf8");

for (const token of [
  "overflow-hidden rounded-lg border border-border-subtle bg-surface",
  "border-b border-border-subtle",
  "px-4 py-2.5",
  "hover:bg-accent",
]) {
  if (!source.includes(token)) {
    throw new Error(`Memory list should use skill list layout token: ${token}`);
  }
}

if (source.includes('<ul className="flex flex-col gap-3">')) {
  throw new Error("Memory list should not use separate card rows");
}
