import { readFileSync } from "node:fs";

const files = [
  "src/pages/agents/AgentPlaza.tsx",
  "src/pages/teams/TeamPlaza.tsx",
];

for (const path of files) {
  const source = readFileSync(path, "utf8");

  for (const required of [
    "group flex flex-col",
    "group-hover:pointer-events-auto",
    "group-hover:opacity-100",
    "group-focus-within:opacity-100",
    "shrink-0 items-center gap-1 rounded-md",
  ]) {
    if (!source.includes(required)) {
      throw new Error(`${path} should show plaza card actions inline on hover: missing ${required}`);
    }
  }

  if (source.includes('className="mt-3 flex justify-end"')) {
    throw new Error(`${path} should not render plaza card actions in a bottom-right footer`);
  }
}

const agentPlazaSource = readFileSync("src/pages/agents/AgentPlaza.tsx", "utf8");

for (const required of [
  "ArrowUpRight",
  "bg-primary px-2 py-1 text-xs font-medium text-primary-foreground",
]) {
  if (!agentPlazaSource.includes(required)) {
    throw new Error(`AgentPlaza should align use button style and icon with mine agent cards: missing ${required}`);
  }
}

if (agentPlazaSource.includes("mt-auto flex justify-end pt-3")) {
  throw new Error("AgentPlaza actions should stay in the title row, not move to the bottom row");
}

if (agentPlazaSource.includes("<Check")) {
  throw new Error("AgentPlaza use button should use ArrowUpRight, not Check");
}
