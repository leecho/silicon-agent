import { show as showApp } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type SystemNotificationInput = {
  body: string;
  onClick?: () => void;
  tag?: string;
  title: string;
};

export async function showSystemNotification(input: SystemNotificationInput): Promise<boolean> {
  if (typeof window === "undefined" || !("Notification" in window)) {
    return false;
  }

  const permission = await ensureNotificationPermission();
  if (permission !== "granted") {
    return false;
  }

  const notification = new Notification(input.title, {
    body: input.body,
    tag: input.tag,
  });
  notification.onclick = () => {
    void handleNotificationClick(notification, input);
  };
  return true;
}

async function handleNotificationClick(
  notification: Notification,
  input: SystemNotificationInput,
): Promise<void> {
  await revealAppWindow();
  input.onClick?.();
  notification.close();
}

async function revealAppWindow(): Promise<void> {
  window.focus();
  try {
    await showApp();
    const appWindow = getCurrentWindow();
    await appWindow.show();
    await appWindow.setFocus();
  } catch (err) {
    console.debug("failed to reveal app window from notification click", err);
  }
}

async function ensureNotificationPermission(): Promise<NotificationPermission> {
  if (Notification.permission === "granted" || Notification.permission === "denied") {
    return Notification.permission;
  }
  return await Notification.requestPermission();
}
