import { readFileSync } from "node:fs";

const appSource = readFileSync("src/App.tsx", "utf8");
const dragRegionSource = readFileSync("src/components/layout/WindowDragRegion.tsx", "utf8");

if (!dragRegionSource.includes("data-tauri-drag-region")) {
  throw new Error("WindowDragRegion should mark DOM nodes as Tauri drag regions");
}

if (!appSource.includes("<WindowDragRegion className=\"fixed inset-x-0 top-0")) {
  throw new Error("App root should render a global top drag region for every page");
}

if (!appSource.includes("z-[15]")) {
  throw new Error("Global drag region should sit above page surfaces but below titlebar action buttons");
}

if (!appSource.includes("<AppShell />")) {
  throw new Error("App root should keep routing inside the global drag region wrapper");
}
