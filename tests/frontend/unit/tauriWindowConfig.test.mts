import { readFileSync } from "node:fs";

function assertEqual<T>(actual: T, expected: T, message: string) {
  if (actual !== expected) {
    throw new Error(`${message}: expected ${String(expected)}, got ${String(actual)}`);
  }
}

const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const mainWindow = config.app.windows.find((window: { title?: string }) => window.title === "SiliconAgent");

if (!mainWindow) {
  throw new Error("main window config not found");
}

assertEqual(mainWindow.trafficLightPosition?.x, 24, "traffic light x position");
assertEqual(mainWindow.trafficLightPosition?.y, 18, "traffic light y position");
