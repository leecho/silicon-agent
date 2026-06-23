import { readFileSync } from "node:fs";

const dragSource = readFileSync("src/components/layout/WindowDragRegion.tsx", "utf8");
const capabilitySource = readFileSync("src-tauri/capabilities/default.json", "utf8");

if (!dragSource.includes('@tauri-apps/api/window')) {
  throw new Error("WindowDragRegion should use Tauri window API for reliable dragging");
}

if (!dragSource.includes("getCurrentWindow")) {
  throw new Error("WindowDragRegion should resolve the current Tauri window");
}

if (!dragSource.includes("startDragging")) {
  throw new Error("WindowDragRegion should explicitly start window dragging on pointer down");
}

if (!dragSource.includes("onPointerDown")) {
  throw new Error("WindowDragRegion should start dragging from pointer down");
}

if (!capabilitySource.includes("core:window:allow-start-dragging")) {
  throw new Error("Tauri capabilities should allow start_dragging");
}
