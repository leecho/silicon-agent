import { existsSync, readFileSync } from "node:fs";

const draftPath = "src/pages/session/SessionDraftPage.tsx";
const barPath = "src/pages/session/PopularExpertsBar.tsx";

if (!existsSync(barPath)) {
  throw new Error("Draft page should have a PopularExpertsBar component above Composer");
}

const draftSource = readFileSync(draftPath, "utf8");
const barSource = readFileSync(barPath, "utf8");

for (const token of [
  "PopularExpertsBar",
  "agents={roleExperts}",
  "seedRole={draftSeedRole}",
  "onPickExpert={(expertId) => void pickRole(\"expert\", expertId)}",
]) {
  if (!draftSource.includes(token)) {
    throw new Error(`SessionDraftPage should wire popular experts into draft role selection: missing ${token}`);
  }
}

const barIndex = draftSource.indexOf("<PopularExpertsBar");
const composerIndex = draftSource.indexOf("<Composer");
if (barIndex < 0 || composerIndex < 0 || barIndex > composerIndex) {
  throw new Error("PopularExpertsBar should render above the Composer on the draft page");
}

for (const token of [
  "Bot",
  "Tooltip",
  "agent.description",
  "onError",
  "slice(0, 6)",
  "activeRoleKind === \"expert\"",
  "activeRoleId === agent.id",
  "seedRole?.kind === \"expert\"",
  "onPickExpert(agent.id)",
]) {
  if (!barSource.includes(token)) {
    throw new Error(`PopularExpertsBar should render selectable common expert chips: missing ${token}`);
  }
}

for (const removed of ["<h2", "agent.profession", "h-14", "min-w-[128px]"]) {
  if (barSource.includes(removed)) {
    throw new Error(`PopularExpertsBar should stay compact and avoid extra copy/metadata: found ${removed}`);
  }
}
