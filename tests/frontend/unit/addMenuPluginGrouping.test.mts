import { readFileSync } from "node:fs";

const addMenuSource = readFileSync("src/pages/session/composer/AddMenu.tsx", "utf8");
const composerSource = readFileSync("src/components/session/Composer.tsx", "utf8");

if (!addMenuSource.includes("pluginNameById")) {
  throw new Error("AddMenu should receive pluginNameById for plugin skill group labels");
}

if (!addMenuSource.includes("function buildSkillMenuEntries")) {
  throw new Error("AddMenu should build grouped skill submenu entries through a helper");
}

if (!addMenuSource.includes("pluginSkillsById")) {
  throw new Error("AddMenu should collect plugin-owned skills by plugin id");
}

if (!addMenuSource.includes('id: `plugin:${pluginId}`')) {
  throw new Error("AddMenu should create a stable custom header entry for each plugin group");
}

if (!addMenuSource.includes("looseSkills")) {
  throw new Error("AddMenu should keep non-plugin skills outside plugin groups");
}

if (!composerSource.includes("listPlugins")) {
  throw new Error("Composer should load plugins so AddMenu can show plugin display names");
}

if (!composerSource.includes("pluginNameById")) {
  throw new Error("Composer should pass pluginNameById into AddMenu");
}
