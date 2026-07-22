import { existsSync, readFileSync } from "node:fs";

const teamsPageSource = readFileSync("src/pages/teams/TeamsPage.tsx", "utf8");
const teamImportModalPath = "src/pages/teams/TeamImportModal.tsx";

if (!existsSync(teamImportModalPath)) {
  throw new Error("Teams page should use a TeamImportModal for importing teams");
}

const teamImportModalSource = readFileSync(teamImportModalPath, "utf8");

if (!teamsPageSource.includes("TeamImportModal")) {
  throw new Error("TeamsPage should render TeamImportModal");
}

if (!teamsPageSource.includes("导入团队")) {
  throw new Error("TeamsPage should expose a single 导入团队 action");
}

for (const legacyLabel of ["导入目录", "导入 zip"]) {
  if (teamsPageSource.includes(legacyLabel)) {
    throw new Error(`TeamsPage should not expose legacy split import action: ${legacyLabel}`);
  }
}

for (const required of [
  "getCurrentWebview",
  "UploadCloud",
  "importTeamFromPath",
  "pickDirectory",
  "pickTeamZip",
  "拖拽 .zip 或团队文件夹到此处",
  "选择 zip",
  "选择文件夹",
]) {
  if (!teamImportModalSource.includes(required)) {
    throw new Error(`TeamImportModal should include ${required}`);
  }
}

