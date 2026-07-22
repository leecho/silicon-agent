import { readFileSync } from "node:fs";

function mustInclude(file: string, tokens: string[]) {
  const source = readFileSync(file, "utf8");
  for (const token of tokens) {
    if (!source.includes(token)) {
      throw new Error(`${file} missing project dedicated skills token: ${token}`);
    }
  }
}

mustInclude("src/api.ts", ["listProjectSkills", '"list_project_skills"']);
mustInclude("src/types.ts", ["ProjectSkill", "sourceKind", "sourceName"]);
mustInclude("src/App.tsx", ["skills: <SkillsPage />"]);
mustInclude("src/pages/projects/ProjectView.tsx", [
  "listProjectSkills",
  "projectSkills",
  "setProjectSkills",
  '"skills"',
  "ProjectSkillList",
  'onGoSkills={() => setView("skills")}',
  "overflow-hidden rounded-lg border border-border-subtle bg-surface",
  "block w-full px-4 py-2.5 text-left transition-colors hover:bg-accent",
  "line-clamp-2 text-xs leading-5 text-foreground-muted [overflow-wrap:anywhere]",
]);

const projectView = readFileSync("src/pages/projects/ProjectView.tsx", "utf8");
if (projectView.includes("grid grid-cols-1 gap-3 sm:grid-cols-2")) {
  throw new Error("Project dedicated skills should use TeamDetailDrawer list rows, not card grid layout");
}
for (const removed of ["属于团队", "属于专家", "属于{sourceLabel}"]) {
  if (projectView.includes(removed)) {
    throw new Error(`Project dedicated skills should show source names as badges, not copy: ${removed}`);
  }
}
mustInclude("src/pages/projects/ProjectHome.tsx", [
  "专属技能",
  "projectSkillCount",
  "onGoSkills",
  "查看全部",
]);
for (const file of ["src/hooks/useAppNavigation.ts", "src/pages/skills/SkillsPage.tsx"]) {
  const source = readFileSync(file, "utf8");
  if (source.includes("projectId") && file.includes("SkillsPage")) {
    throw new Error("SkillsPage should not own project dedicated skill routing");
  }
  if (source.includes('{ section: "skills"; projectId?: string | null }')) {
    throw new Error("App navigation should not route project skills through global SkillsPage");
  }
}
