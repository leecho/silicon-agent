import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/scheduling/ExecutionTimeline.tsx", "utf8");

if (!source.includes('<ul className="overflow-hidden rounded-lg border border-border-subtle bg-card">')) {
  throw new Error("ExecutionTimeline should render executions in a bordered outline list like SkillsPage");
}

if (!source.includes("<li") || !source.includes("border-b border-border-subtle")) {
  throw new Error("ExecutionTimeline rows should use list items with subtle dividers");
}

if (!source.includes("grid h-10 w-10 shrink-0 place-items-center rounded-lg border border-border bg-background shadow-sm")) {
  throw new Error("ExecutionTimeline should use the same leading outline icon block as the skills list");
}

if (!source.includes('completed: "border border-border bg-muted text-foreground"')) {
  throw new Error("Completed executions should use a neutral badge, not primary emphasis");
}

if (!source.includes('needs_attention: "border border-border bg-muted text-foreground"')) {
  throw new Error("Needs-attention executions should use a neutral badge");
}

if (!source.includes('skipped: "bg-muted text-foreground-muted"')) {
  throw new Error("Skipped executions should use muted badge text");
}

if (!source.includes('completed: "text-foreground"') || !source.includes('needs_attention: "text-foreground"')) {
  throw new Error("Completed and needs-attention icons should use neutral foreground");
}

if (source.includes('className="flex flex-col gap-2"')) {
  throw new Error("ExecutionTimeline should not render execution rows as separated card buttons");
}
