import { AlertTriangle, CheckCircle2, Info, XCircle } from "lucide-react";
import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from "react";
import { Button } from "./Button";
import { ButtonGroup } from "./ButtonGroup";
import { Modal } from "./Modal";
import { joinClasses } from "./utils";

type MessageTone = "info" | "success" | "warning" | "error";
type DialogKind = "alert" | "confirm" | "prompt";

type MessageOptions = {
  message: ReactNode;
  title?: string;
};

type DialogOptions = MessageOptions & {
  cancelText?: string;
  confirmText?: string;
  defaultValue?: string;
  placeholder?: string;
  tone?: MessageTone;
};

type MessageInput = ReactNode | MessageOptions;

type DialogState = DialogOptions & {
  id: string;
  kind: DialogKind;
  resolve: (value: unknown) => void;
};

type MessagesApi = {
  alert: (input: MessageInput | DialogOptions) => Promise<void>;
  confirm: (input: MessageInput | DialogOptions) => Promise<boolean>;
  error: (input: MessageInput | DialogOptions) => Promise<void>;
  info: (input: MessageInput | DialogOptions) => Promise<void>;
  prompt: (input: MessageInput | DialogOptions) => Promise<string | null>;
  success: (input: MessageInput | DialogOptions) => Promise<void>;
  warning: (input: MessageInput | DialogOptions) => Promise<void>;
};

const MessageContext = createContext<MessagesApi | null>(null);

export function MessageProvider({ children }: { children: ReactNode }) {
  const [dialog, setDialog] = useState<DialogState | null>(null);
  const [promptValue, setPromptValue] = useState("");
  const activeDialogRef = useRef(false);
  const dialogQueueRef = useRef<DialogState[]>([]);

  const showNextDialog = useCallback(() => {
    if (activeDialogRef.current) return;
    const next = dialogQueueRef.current.shift();
    if (!next) return;
    activeDialogRef.current = true;
    setPromptValue(next.defaultValue ?? "");
    setDialog(next);
  }, []);

  const enqueueDialog = useCallback(
    <T,>(kind: DialogKind, input: MessageInput | DialogOptions, fallbackTone: MessageTone) => {
      return new Promise<T>((resolve) => {
        dialogQueueRef.current.push({
          ...normalizeDialogOptions(input, fallbackTone),
          id: createMessageId(),
          kind,
          resolve: resolve as (value: unknown) => void
        });
        showNextDialog();
      });
    },
    [showNextDialog]
  );

  const closeDialog = useCallback(
    (value: unknown) => {
      if (!dialog) return;
      dialog.resolve(value);
      activeDialogRef.current = false;
      setDialog(null);
      setPromptValue("");
      window.setTimeout(showNextDialog, 0);
    },
    [dialog, showNextDialog]
  );

  const api = useMemo<MessagesApi>(
    () => ({
      alert: (input) => enqueueDialog<void>("alert", input, "info"),
      confirm: (input) => enqueueDialog<boolean>("confirm", input, "warning"),
      error: (input) => enqueueDialog<void>("alert", input, "error"),
      info: (input) => enqueueDialog<void>("alert", input, "info"),
      prompt: (input) => enqueueDialog<string | null>("prompt", input, "info"),
      success: (input) => enqueueDialog<void>("alert", input, "success"),
      warning: (input) => enqueueDialog<void>("alert", input, "warning")
    }),
    [enqueueDialog]
  );

  return (
    <MessageContext.Provider value={api}>
      {children}
      <MessageDialog
        dialog={dialog}
        promptValue={promptValue}
        onCancel={() => closeDialog(dialog?.kind === "confirm" ? false : dialog?.kind === "prompt" ? null : undefined)}
        onConfirm={() => closeDialog(dialog?.kind === "confirm" ? true : dialog?.kind === "prompt" ? promptValue : undefined)}
        onPromptValueChange={setPromptValue}
      />
    </MessageContext.Provider>
  );
}

export function useMessages() {
  const context = useContext(MessageContext);
  if (!context) throw new Error("useMessages must be used within MessageProvider");
  return context;
}

function MessageDialog({
  dialog,
  onCancel,
  onConfirm,
  onPromptValueChange,
  promptValue
}: {
  dialog: DialogState | null;
  onCancel: () => void;
  onConfirm: () => void;
  onPromptValueChange: (value: string) => void;
  promptValue: string;
}) {
  if (!dialog) return null;

  return (
    <Modal className="w-[380px]" open title={dialog.title} onClose={onCancel}>
      <div className="grid grid-cols-[auto_minmax(0,1fr)] gap-3">
        <ToneIcon className="mt-1 h-5 w-5" tone={dialog.tone ?? "info"} />
        <div className="min-w-0">
          <h2 className="text-base font-semibold text-foreground">{dialog.title ?? defaultDialogTitle(dialog.kind, dialog.tone)}</h2>
          <div className="mt-2 text-[13px] text-foreground-secondary">{dialog.message}</div>
          {dialog.kind === "prompt" && (
            <input
              autoFocus
              className="mt-2 w-full rounded-sm border border-input bg-background px-2 py-2 text-sm text-foreground outline-none transition placeholder:text-foreground-muted focus:border-ring"
              placeholder={dialog.placeholder}
              value={promptValue}
              onChange={(event) => onPromptValueChange(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") onConfirm();
              }}
            />
          )}
        </div>
      </div>
      <ButtonGroup className="mt-5">
        {dialog.kind !== "alert" && (
          <Button tone="outline" onClick={onCancel}>
            {dialog.cancelText ?? "取消"}
          </Button>
        )}
        <Button tone={dialog.tone === "error" || dialog.tone === "warning" ? "danger" : "primary"} onClick={onConfirm}>
          {dialog.confirmText ?? (dialog.kind === "alert" ? "知道了" : "确认")}
        </Button>
      </ButtonGroup>
    </Modal>
  );
}

function ToneIcon({ className, tone }: { className?: string; tone: MessageTone }) {
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

function normalizeMessageOptions(input: MessageInput): MessageOptions {
  if (isMessageOptions(input)) return input;
  return { message: input as ReactNode };
}

function normalizeDialogOptions(input: MessageInput | DialogOptions, fallbackTone: MessageTone): DialogOptions {
  const options = normalizeMessageOptions(input);
  return {
    ...options,
    tone: isMessageOptions(input) && "tone" in input ? input.tone ?? fallbackTone : fallbackTone
  };
}

function isMessageOptions(input: MessageInput | DialogOptions): input is DialogOptions {
  return Boolean(input && typeof input === "object" && "message" in input);
}

function defaultDialogTitle(kind: DialogKind, tone?: MessageTone) {
  if (kind === "confirm") return "需要确认";
  if (kind === "prompt") return "请输入";
  if (tone === "error") return "错误";
  if (tone === "warning") return "警告";
  return "提示";
}

function createMessageId() {
  return `message-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
