import { readFileSync } from "node:fs";

const drawerSource = readFileSync("src/components/ui/Drawer.tsx", "utf8");
const pluginSource = readFileSync("src/pages/plugins/PluginDetailDrawer.tsx", "utf8");
const teamSource = readFileSync("src/pages/teams/TeamDetailDrawer.tsx", "utf8");
const catalogTeamSource = readFileSync("src/pages/teams/CatalogTeamDrawer.tsx", "utf8");

for (const token of [
  "width?: string",
  "width,",
  "widthClassName = \"w-[min(980px,92vw)]\"",
  "width ? null : widthClassName",
  "style={width ? { width } : undefined}",
]) {
  if (!drawerSource.includes(token)) {
    throw new Error(`Drawer should expose explicit inline width to avoid conflicting Tailwind width utilities: missing ${token}`);
  }
}

for (const [name, source, width] of [
  ["PluginDetailDrawer", pluginSource, 'width="min(720px, 94vw)"'],
  ["TeamDetailDrawer", teamSource, 'width="min(640px, 94vw)"'],
  ["CatalogTeamDrawer", catalogTeamSource, 'width="min(640px, 94vw)"'],
] as const) {
  if (!source.includes(width)) {
    throw new Error(`${name} should pass its drawer width through the explicit width prop: missing ${width}`);
  }
}

for (const [name, source] of [
  ["PluginDetailDrawer", pluginSource],
  ["TeamDetailDrawer", teamSource],
  ["CatalogTeamDrawer", catalogTeamSource],
] as const) {
  if (source.includes('className="w-[')) {
    throw new Error(`${name} should not mix width utilities into Drawer className`);
  }
  if (source.includes("widthClassName=")) {
    throw new Error(`${name} should not use Tailwind width classes for drawer sizing`);
  }
}
