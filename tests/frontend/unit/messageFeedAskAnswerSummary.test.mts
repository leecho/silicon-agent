import { readFileSync } from "node:fs";

const source = readFileSync("src/components/session/MessageFeed.tsx", "utf8");

for (const required of [
  "function parseAskAnswerRows",
  "function AskAnswerSummary",
  "问答摘要",
  "grid-cols-[minmax(96px,0.38fr)_minmax(0,1fr)]",
  "first:border-t-0",
  "question",
  "answer",
]) {
  if (!source.includes(required)) {
    throw new Error(`MessageFeed should render ask_user answers as a structured summary: missing ${required}`);
  }
}

if (source.includes("用户对反问的回答：右对齐气泡")) {
  throw new Error("Ask answers should not be rendered as regular right-aligned user bubbles");
}

if (!source.includes('elements.push(<AskAnswerSummary key={row.id} content={row.content} />);')) {
  throw new Error("Ask answers should be rendered through AskAnswerSummary");
}

if (source.includes("space-y-2") && source.includes("bg-background/70 px-3 py-2")) {
  throw new Error("Ask answer summary should use a compact row list instead of separate cards");
}
