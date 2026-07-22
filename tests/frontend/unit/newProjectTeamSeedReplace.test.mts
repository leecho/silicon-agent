import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/projects/NewProjectModal.tsx", "utf8");

for (const required of [
  "useMessages",
  "messages.confirm",
  "导入并替换",
  "setSeededMembers(d.members)",
  "setSeededFrom(next)",
  "setMembers(d.members.map((m) => m.name))",
]) {
  if (!source.includes(required)) {
    throw new Error(`NewProjectModal should replace members after confirmed team import: missing ${required}`);
  }
}

for (const legacy of [
  "const names = new Set(prev)",
  "return [...names]",
  "const byName = new Map(prev.map((a) => [a.name, a]))",
]) {
  if (source.includes(legacy)) {
    throw new Error(`NewProjectModal should not merge team members into existing draft members: ${legacy}`);
  }
}
