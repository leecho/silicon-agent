import { AlertTriangle, CheckCircle2, Info, X, XCircle } from "lucide-react";
import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from "react";
import { joinClasses } from "./utils";

type NotificationTone = "info" | "success" | "warning" | "error";

type NotificationOptions = {
  action?: {
    label: string;
    onClick: () => void;
  };
  duration?: number;
  message: ReactNode;
  title?: string;
  tone?: NotificationTone;
};

type NotificationInput = ReactNode | NotificationOptions;

type NotificationItem = NotificationOptions & {
  id: string;
  tone: NotificationTone;
};

type NotificationsApi = {
  clear: () => void;
  dismiss: (id: string) => void;
  error: (input: NotificationInput) => string;
  info: (input: NotificationInput) => string;
  notify: (input: NotificationInput) => string;
  success: (input: NotificationInput) => string;
  warning: (input: NotificationInput) => string;
};

const NotificationContext = createContext<NotificationsApi | null>(null);
const DEFAULT_DURATION = 4200;
const MAX_VISIBLE_NOTIFICATIONS = 5;

export function NotificationProvider({ children }: { children: ReactNode }) {
  const [notifications, setNotifications] = useState<NotificationItem[]>([]);
  const timersRef = useRef(new Map<string, number>());

  const dismiss = useCallback((id: string) => {
    const timer = timersRef.current.get(id);
    if (timer) window.clearTimeout(timer);
    timersRef.current.delete(id);
    setNotifications((items) => items.filter((item) => item.id !== id));
  }, []);

  const clear = useCallback(() => {
    for (const timer of timersRef.current.values()) {
      window.clearTimeout(timer);
    }
    timersRef.current.clear();
    setNotifications([]);
  }, []);

  const push = useCallback(
    (tone: NotificationTone, input: NotificationInput) => {
      const options = normalizeNotificationOptions(input, tone);
      const id = createNotificationId();
      setNotifications((items) => [...items, { ...options, id, tone: options.tone ?? tone }].slice(-MAX_VISIBLE_NOTIFICATIONS));

      if (options.duration !== 0) {
        const timer = window.setTimeout(() => dismiss(id), options.duration ?? DEFAULT_DURATION);
        timersRef.current.set(id, timer);
      }

      return id;
    },
    [dismiss]
  );

  const api = useMemo<NotificationsApi>(
    () => ({
      clear,
      dismiss,
      error: (input) => push("error", input),
      info: (input) => push("info", input),
      notify: (input) => push("info", input),
      success: (input) => push("success", input),
      warning: (input) => push("warning", input)
    }),
    [clear, dismiss, push]
  );

  return (
    <NotificationContext.Provider value={api}>
      {children}
      <NotificationViewport notifications={notifications} onDismiss={dismiss} />
    </NotificationContext.Provider>
  );
}

export function useNotifications() {
  const context = useContext(NotificationContext);
  if (!context) throw new Error("useNotifications must be used within NotificationProvider");
  return context;
}

function NotificationViewport({
  notifications,
  onDismiss
}: {
  notifications: NotificationItem[];
  onDismiss: (id: string) => void;
}) {
  if (notifications.length === 0) return null;

  return (
    <div className="fixed right-5 top-5 z-[90] grid w-[min(390px,calc(100vw-32px))] gap-2">
      {notifications.map((notification) => (
        <article
          className={joinClasses(
            "grid min-w-0 grid-cols-[auto_minmax(0,1fr)_auto] gap-3 rounded-lg border border-border bg-popover px-3 py-3 text-popover-foreground shadow-2xl",
            toneBorderClass(notification.tone)
          )}
          key={notification.id}
          role="status"
        >
          <ToneIcon className="mt-0.5 h-4 w-4 shrink-0" tone={notification.tone} />
          <div className="min-w-0">
            {notification.title && <h3 className="truncate text-sm font-semibold text-popover-foreground">{notification.title}</h3>}
            <div className="break-words text-sm leading-6 text-foreground-secondary">{notification.message}</div>
            {notification.action && (
              <button
                className="mt-2 rounded-md bg-accent px-2 py-1 text-xs font-semibold text-accent-foreground transition hover:brightness-110"
                type="button"
                onClick={() => {
                  notification.action?.onClick();
                  onDismiss(notification.id);
                }}
              >
                {notification.action.label}
              </button>
            )}
          </div>
          <button
            className="grid h-7 w-7 place-items-center rounded-lg text-foreground-muted transition hover:bg-accent hover:text-accent-foreground"
            type="button"
            aria-label="关闭通知"
            onClick={() => onDismiss(notification.id)}
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </article>
      ))}
    </div>
  );
}

function ToneIcon({ className, tone }: { className?: string; tone: NotificationTone }) {
  const iconClass = joinClasses(
    tone === "error"
      ? "text-danger"
      : tone === "warning"
        ? "text-warning"
        : tone === "success"
          ? "text-success"
          : "text-primary",
    className
  );
  if (tone === "error") return <XCircle className={iconClass} aria-hidden="true" />;
  if (tone === "warning") return <AlertTriangle className={iconClass} aria-hidden="true" />;
  if (tone === "success") return <CheckCircle2 className={iconClass} aria-hidden="true" />;
  return <Info className={iconClass} aria-hidden="true" />;
}

function normalizeNotificationOptions(input: NotificationInput, tone: NotificationTone): NotificationOptions {
  if (isNotificationOptions(input)) return { ...input, tone: input.tone ?? tone };
  return { message: input as ReactNode, tone };
}

function isNotificationOptions(input: NotificationInput): input is NotificationOptions {
  return Boolean(input && typeof input === "object" && "message" in input);
}

function toneBorderClass(tone: NotificationTone) {
  if (tone === "error") return "border-danger-border";
  if (tone === "warning") return "border-warning-border";
  if (tone === "success") return "border-success-border";
  return "border-border";
}

function createNotificationId() {
  return `notification-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
