import { existsSync, readFileSync } from "node:fs";

const files = {
  app: readFileSync("src/App.tsx", "utf8"),
  sidebar: readFileSync("src/components/layout/Sidebar.tsx", "utf8"),
  sessionManager: readFileSync("src/components/layout/SessionManager.tsx", "utf8"),
  normalSessions: readFileSync("src/components/layout/session-manager/NormalSessions.tsx", "utf8"),
  projectsPage: readFileSync("src/pages/projects/ProjectsPage.tsx", "utf8"),
};

const projectTreePath = "src/components/layout/session-manager/ProjectSessions.tsx";
if (!existsSync(projectTreePath)) {
  throw new Error("SessionManager should add a ProjectSessions tree component");
}
const projectSessionActionMenuPath = "src/components/layout/session-manager/ProjectSessionActionMenu.tsx";
if (!existsSync(projectSessionActionMenuPath)) {
  throw new Error("Project session rows should have a dedicated action menu");
}

const projectTree = readFileSync(projectTreePath, "utf8");
const projectSessionActionMenu = readFileSync(projectSessionActionMenuPath, "utf8");

for (const token of [
  "ProjectSessions",
  "listProjects",
  "listProjectSessions",
  "onNewProjectSession",
  'from "./SessionRows"',
  "<GroupRow",
  "<ItemRow",
  "renderProjectSessionActions",
  "onOpenProjectSessionMenu",
  "MoreHorizontal",
  "session.pinned",
  "项目",
  'aria-label="新增项目"',
  'aria-label="项目列表"',
  'aria-label={`查看项目：${project.name}`}',
  'aria-label={`新增项目会话：${project.name}`}',
  "group-hover:opacity-100",
  "group-focus-within:opacity-100",
]) {
  if (!projectTree.includes(token)) {
    throw new Error(`ProjectSessions should include ${token}`);
  }
}

if (projectTree.includes('<Tooltip content="更多">')) {
  throw new Error("Project session row actions should not show a visible 更多 tooltip bubble");
}

for (const token of ["const active = session.id === currentSessionId", "hover:bg-white/15", "hover:bg-accent"]) {
  if (!projectTree.includes(token)) {
    throw new Error(`Project session more button should style against active rows: missing ${token}`);
  }
}

for (const forbidden of ["DropdownMenu", "menuTarget", "openMenu"]) {
  if (projectTree.includes(forbidden)) {
    throw new Error(`Project actions should be flat buttons, not a dropdown menu: found ${forbidden}`);
  }
}

for (const token of ["ProjectSessionActionMenu", "DropdownMenu", "重命名", "置顶", "取消置顶", "删除"]) {
  if (!projectSessionActionMenu.includes(token)) {
    throw new Error(`Project session action menu should support rename/delete/pin: missing ${token}`);
  }
}

if (!projectTree.includes("project.id") || !projectTree.includes("project.name")) {
  throw new Error("ProjectSessions should group sessions under real project ids and names");
}

for (const token of [
  "ProjectSessions",
  "onOpenProject",
  "onOpenProjectList",
  "onCreateProject",
  "NewProjectModal",
  "ProjectSessionActionMenu",
  "projectCreateOpen",
  "projectMenuSession",
  "handleRename",
  "handleDelete",
  "handleTogglePinned",
  "enterDraftWithProject",
]) {
  if (!files.sessionManager.includes(token)) {
    throw new Error(`SessionManager should compose project sessions with ${token}`);
  }
}

if (!files.sessionManager.includes("<ProjectSessions")) {
  throw new Error("SessionManager should render the project/session combined tree in the default view");
}

for (const legacy of ["createProject,", 'title: "新增项目"', 'message: "输入项目名称"']) {
  if (files.sessionManager.includes(legacy)) {
    throw new Error(`SessionManager should open the project creation drawer instead of prompt-creating projects: found ${legacy}`);
  }
}

if (!files.normalSessions.includes("会话")) {
  throw new Error("NormalSessions should preserve the original session list under 会话/{分组}/{会话}");
}

for (const token of [
  "onOpenProject",
  "onOpenProjectList",
  "onCreateProject",
  "SessionManager",
]) {
  if (!files.sidebar.includes(token)) {
    throw new Error(`Sidebar should pass project actions to SessionManager: missing ${token}`);
  }
}

for (const token of [
  "handleOpenProject",
  "handleOpenProjectList",
  'onNavigate({ section: "projects", projectId })',
  'projectId={location.section === "projects" ? location.projectId ?? null : null}',
]) {
  if (!files.app.includes(token)) {
    throw new Error(`App should route SessionManager project actions into ProjectsPage: missing ${token}`);
  }
}

for (const token of [
  "projectId",
  "useEffect",
  "setOpenId(projectId)",
]) {
  if (!files.projectsPage.includes(token)) {
    throw new Error(`ProjectsPage should accept directed project opening: missing ${token}`);
  }
}
