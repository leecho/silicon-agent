import { readFileSync } from "node:fs";

const source = readFileSync("src/components/session/SessionAskCard.tsx", "utf8");

for (const required of [
  "freeTextActive",
  "freeTextOptionClassName",
  "aria-label=\"其他自由补充\"",
  "bg-transparent",
  "placeholder={visibleOptions.length > 0 ? \"其他（自由补充）\" : \"在此输入你的回答\"}",
]) {
  if (!source.includes(required)) {
    throw new Error(`SessionAskCard should render freeform input as an integrated option row: missing ${required}`);
  }
}

if (source.includes('className="h-11 min-w-0 rounded border border-border-subtle bg-card')) {
  throw new Error("SessionAskCard should not render the freeform input as a standalone mismatched field");
}

