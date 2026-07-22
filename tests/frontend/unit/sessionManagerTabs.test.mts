import { existsSync, readFileSync } from "node:fs";

const source = readFileSync("src/components/layout/SessionManager.tsx", "utf8");

for (const token of ["SESSION_MANAGER_TABS", "activeTab", 'role="tablist"', 'role="tab"', "ScheduledTaskSessions"]) {
  if (source.includes(token)) {
    throw new Error(`SessionManager should remove tabbed panels and scheduled-task sidebar entry: found ${token}`);
  }
}

for (const token of ["ProjectSessions", "NormalSessions", "RemoteSessions"]) {
  if (!source.includes(token)) {
    throw new Error(`SessionManager should render ${token} in the default tree view`);
  }
}

const projectSource = readFileSync("src/components/layout/session-manager/ProjectSessions.tsx", "utf8");
const agentSource = readFileSync("src/components/layout/session-manager/AgentSessions.tsx", "utf8");
const normalSource = readFileSync("src/components/layout/session-manager/NormalSessions.tsx", "utf8");
const remoteSource = readFileSync("src/components/layout/session-manager/RemoteSessions.tsx", "utf8");
const sharedSource = readFileSync("src/components/layout/session-manager/sessionManagerShared.tsx", "utf8");
const dataSource = readFileSync("src/components/layout/session-manager/useSessionManagerData.ts", "utf8");
const rowsPath = "src/components/layout/session-manager/SessionRows.tsx";

if (!existsSync(rowsPath)) {
  throw new Error("GroupRow and ItemRow should be shared from SessionRows.tsx");
}

const rowsSource = readFileSync(rowsPath, "utf8");

for (const token of ["!s.isDraft", "DraftSessions"]) {
  if (normalSource.includes(token) || source.includes(token)) {
    throw new Error(`Drafts should be merged into the normal session groups, not split by ${token}`);
  }
}

for (const token of ["isTopLevelSession", "!session.parentSessionId"]) {
  if (!dataSource.includes(token)) {
    throw new Error(`SessionManager data should hide child sessions from sidebar lists: missing ${token}`);
  }
}

for (const sourceText of [projectSource, agentSource, normalSource, remoteSource]) {
  for (const forbidden of ["SessionTreeContent", "SessionTreeNode", "SessionManagerTemplate"]) {
    if (sourceText.includes(forbidden)) {
      throw new Error(`Project/session/remote roots should not use tree-node rendering: found ${forbidden}`);
    }
  }
  if (sourceText.includes("function SectionTitle") || sourceText.includes("<SectionTitle")) {
    throw new Error("Project/session/remote components should inline title DOM, not define SectionTitle");
  }
  for (const localRow of ["function GroupRow", "function ItemRow"]) {
    if (sourceText.includes(localRow)) {
      throw new Error(`Project/session/remote components should import shared rows, not define ${localRow}`);
    }
  }
  for (const rowToken of ['from "./SessionRows"', "<GroupRow", "<ItemRow"]) {
    if (!sourceText.includes(rowToken)) {
      throw new Error(`Project/session/remote components should use shared row components: missing ${rowToken}`);
    }
  }
}

for (const token of ["export function GroupRow", "export function ItemRow"]) {
  if (!rowsSource.includes(token)) {
    throw new Error(`SessionRows should export shared row component: missing ${token}`);
  }
}

for (const standard of [
  "group relative flex h-[35px] w-full rounded-sm cursor-pointer items-center gap-1.5 px-2.5 py-2.5 text-left text-foreground-secondary transition hover:bg-card",
  "absolute right-2.5 top-1/2 -translate-y-1/2",
  "block min-w-0 truncate text-[13px] uppercase leading-none text-foreground-secondary",
  'text-foreground hover:bg-accent hover:text-accent-foreground',
  "group relative text-[13px] flex h-[34px] w-full cursor-pointer items-center gap-1.5 rounded-sm py-2.5 text-left transition",
  "absolute right-1 top-1/2 -translate-y-1/2",
  "opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100",
]) {
  if (!rowsSource.includes(standard)) {
    throw new Error(`Shared rows should preserve the ProjectSessions DOM standard: missing ${standard}`);
  }
}

for (const sourceText of [projectSource, agentSource, normalSource, remoteSource]) {
  for (const standard of [
    "group flex h-7 cursor-pointer items-center gap-1.5 px-2 text-foreground-muted",
    "opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100",
  ]) {
    if (!sourceText.includes(standard)) {
      throw new Error(`Project/session/remote components should share the ProjectSessions DOM standard: missing ${standard}`);
    }
  }
}

for (const sourceText of [projectSource, agentSource, normalSource, remoteSource]) {
  if (!sourceText.includes('return (\n    <div className="flex flex-col gap-0.5">')) {
    throw new Error("Project/agent/session/remote roots should use the same section wrapper for consistent spacing");
  }
}

for (const token of ["__projects_root__", "__sessions_root__", "__remote_root__"]) {
  if (projectSource.includes(token) || normalSource.includes(token) || remoteSource.includes(token)) {
    throw new Error(`Top-level ${token} should not be represented as a SessionTree node`);
  }
}

if (sharedSource.includes("function SessionManagerSectionTitle")) {
  throw new Error("SessionManagerSectionTitle should not live in shared helpers");
}

for (const token of ["expanded", "onToggle", "ChevronRight"]) {
  for (const sourceText of [projectSource, normalSource, remoteSource]) {
    if (!sourceText.includes(token)) {
      throw new Error(`Local section titles should support first-level expand/collapse: missing ${token}`);
    }
  }
}

for (const sourceText of [projectSource, normalSource, remoteSource]) {
  if (!sourceText.includes("expanded=") || !sourceText.includes("onToggle=")) {
    throw new Error("Project/session/remote section titles should wire expand/collapse state");
  }
}

for (const token of ["draftTitle", "FileText", "onOpenDraft", "!s.projectId"]) {
  if (!normalSource.includes(token)) {
    throw new Error(`NormalSessions should render grouped drafts with a draft icon: missing ${token}`);
  }
}

for (const token of ["远程", "remoteChannels", "remoteChannelIcon"]) {
  if (!remoteSource.includes(token)) {
    throw new Error(`RemoteSessions should render 远程/{IM}/{会话}: missing ${token}`);
  }
}
