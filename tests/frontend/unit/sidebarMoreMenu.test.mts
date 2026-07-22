import { readFileSync } from "node:fs";

const source = readFileSync("src/components/layout/Sidebar.tsx", "utf8");
const dropdownSource = readFileSync("src/components/ui/DropdownMenu.tsx", "utf8");

for (const required of [
  "MORE_NAV_IDS",
  "moreNavItems",
  "topNavItems",
  "openMoreMenu",
  "scheduleCloseMoreMenu",
  "更多",
  "<DropdownMenu",
]) {
  if (!source.includes(required)) {
    throw new Error(`Sidebar should group secondary nav in a More menu: missing ${required}`);
  }
}

for (const id of ['"plugins"', '"remote"', '"scheduling"']) {
  if (!source.includes(`MORE_NAV_IDS.includes(item.id)`)) {
    throw new Error("Sidebar top nav should filter More menu ids out of direct nav items");
  }
  if (!source.includes(id)) {
    throw new Error(`Sidebar More menu should include ${id}`);
  }
}

if (!source.includes("MORE_NAV_IDS.includes(activeSection)")) {
  throw new Error("Sidebar More menu should stay active when a grouped section is selected");
}

if (!source.includes("onMouseEnter={openMoreMenu}") || !source.includes("onMouseLeave={scheduleCloseMoreMenu}")) {
  throw new Error("Sidebar More menu should open on hover and close when the pointer leaves");
}

if (!source.includes("position={morePosition}") || !source.includes("sidebarRef.current?.getBoundingClientRect()")) {
  throw new Error("Sidebar More menu should be positioned from the aside right edge");
}

if (!dropdownSource.includes("onMouseEnter?: () => void") || !dropdownSource.includes("onMouseLeave?: () => void")) {
  throw new Error("DropdownMenu should expose mouse enter/leave hooks for hover flyouts");
}
