import { readFileSync } from "node:fs";

const draftSource = readFileSync("src/pages/session/SessionDraftPage.tsx", "utf8");

for (const required of [
  "const seedAgentAppliedRef = useRef(false)",
  "if (draftSeedAgentId && !seedAgentAppliedRef.current)",
  "void pickAgent(draftSeedAgentId)",
]) {
  if (!draftSource.includes(required)) {
    throw new Error(`SessionDraftPage should persist seeded agent drafts before submit: missing ${required}`);
  }
}

if (!draftSource.includes("setDraftSession((prev) => (prev ? { ...prev, agentId: id || null } : prev))")) {
  throw new Error("SessionDraftPage should keep local draft session agentId in sync after persisting it.");
}
