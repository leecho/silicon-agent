import { readFileSync } from "node:fs";

const source = readFileSync("src/components/ui/DropdownMenu.tsx", "utf8");

if (!source.includes("createPortal")) {
  throw new Error("DropdownMenu should render through a portal to avoid sidebar clipping");
}

if (!source.includes("document.body")) {
  throw new Error("DropdownMenu portal should target document.body");
}

if (!source.includes("anchorElement") || !source.includes("createPortal(submenu, document.body)")) {
  throw new Error("DropdownSubMenu should render through a body portal anchored to the parent item");
}

if (!source.includes("window.setTimeout(() => setSubOpen(false)")) {
  throw new Error("Dropdown submenu hover should remain open while moving from the parent item into the portal");
}

if (!source.includes('closest("[data-dropdown-menu-portal]")')) {
  throw new Error("Dropdown outside-click handling should treat submenu portals as part of the menu");
}
