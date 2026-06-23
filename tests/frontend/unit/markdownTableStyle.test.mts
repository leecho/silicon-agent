import { readFileSync } from "node:fs";

const source = readFileSync("src/components/ui/MarkdownText.tsx", "utf8");

const requiredSnippets = [
  "rounded-md border border-border bg-surface",
  "border-separate border-spacing-0",
  "border-b border-r border-border-subtle",
  "border-r border-t border-border-subtle",
  "last:border-r-0",
];

for (const snippet of requiredSnippets) {
  if (!source.includes(snippet)) {
    throw new Error(`Markdown tables should render as rounded bordered rectangles: missing ${snippet}`);
  }
}

if (source.includes("border-collapse")) {
  throw new Error("Markdown tables should not use border-collapse because it breaks rounded corners");
}
