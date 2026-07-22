import { readFileSync } from "node:fs";

const sessionManagerSource = readFileSync("src/components/layout/SessionManager.tsx", "utf8");
const panelSources = [
  "src/components/layout/session-manager/ProjectSessions.tsx",
  "src/components/layout/session-manager/NormalSessions.tsx",
  "src/components/layout/session-manager/DraftSessions.tsx",
  "src/components/layout/session-manager/RemoteSessions.tsx",
].map((path) => [path, readFileSync(path, "utf8")] as const);

for (const [path, source] of panelSources) {
  for (const forbidden of ["SessionTreeContent", "SessionTreeNode", "../../session-tree/SessionTree", "SessionManagerTemplate"]) {
    if (source.includes(forbidden)) {
      throw new Error(`${path} should not depend on tree-node rendering: found ${forbidden}`);
    }
  }
  if (source.includes("function SectionTitle") || source.includes("<SectionTitle")) {
    throw new Error(`${path} should inline section title DOM instead of using a SectionTitle component`);
  }
  if (path.endsWith("DraftSessions.tsx")) continue;
  for (const localRow of ["function GroupRow", "function ItemRow"]) {
    if (source.includes(localRow)) {
      throw new Error(`${path} should import shared row components instead of defining ${localRow}`);
    }
  }
  for (const rowToken of ['from "./SessionRows"', "<GroupRow", "<ItemRow"]) {
    if (!source.includes(rowToken)) {
      throw new Error(`${path} should render shared DOM row components: missing ${rowToken}`);
    }
  }
}

for (const stateName of ["normalTreeOpen", "draftTreeOpen", "remoteTreeOpen"]) {
  if (sessionManagerSource.includes(stateName)) {
    throw new Error(`SessionManager should not keep first-level tree state ${stateName}`);
  }
}
