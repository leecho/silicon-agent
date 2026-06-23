import { readFileSync } from "node:fs";

const bridgeSource = readFileSync("src/components/session/SessionAttentionNotificationBridge.tsx", "utf8");
const notificationSource = readFileSync("src/lib/systemNotifications.ts", "utf8");
const appSource = readFileSync("src/App.tsx", "utf8");

for (const required of [
  "subscribeAgentStreamEvents",
  'e.kind === "permission_required"',
  'e.kind === "ask_required"',
  "buildSessionAttentionNotification",
  "showSystemNotification",
  "openSession(targetSessionId)",
]) {
  if (!bridgeSource.includes(required)) {
    throw new Error(`Session attention notification bridge should include ${required}`);
  }
}

if (!bridgeSource.includes("const targetSessionId = e.sessionId")) {
  throw new Error("Child ask/permission notifications should target the child session directly");
}

if (!bridgeSource.includes("targetSessionId === currentSessionIdRef.current")) {
  throw new Error("Desktop notifications should be skipped only when the target session is already open");
}

for (const required of [
  "@tauri-apps/api/app",
  "@tauri-apps/api/window",
  "showApp",
  "getCurrentWindow",
  "new Notification",
  "notification.onclick",
  "revealAppWindow",
  ".show()",
  ".setFocus()",
  "window.focus",
  "requestPermission",
]) {
  if (!notificationSource.includes(required)) {
    throw new Error(`System notification helper should include ${required}`);
  }
}

if (!appSource.includes("<SessionAttentionNotificationBridge />")) {
  throw new Error("App should mount the global session attention notification bridge");
}
