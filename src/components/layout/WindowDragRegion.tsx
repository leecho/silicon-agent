import { getCurrentWindow } from "@tauri-apps/api/window";
import type { PointerEvent } from "react";

export const WINDOW_DRAG_REGION_PROPS = {
  "data-tauri-drag-region": ""
} as const;

export function WindowDragRegion({ className = "h-7" }: { className?: string }) {
  async function startDragging(event: PointerEvent<HTMLDivElement>) {
    if (event.button !== 0) return;
    event.preventDefault();
    try {
      await getCurrentWindow().startDragging();
    } catch {
      // Browser preview has no Tauri window; native drag is only available in the desktop shell.
    }
  }

  return (
    <div
      {...WINDOW_DRAG_REGION_PROPS}
      aria-hidden="true"
      className={`absolute inset-x-0 top-0 z-10 cursor-default ${className}`}
      onPointerDown={startDragging}
    />
  );
}
