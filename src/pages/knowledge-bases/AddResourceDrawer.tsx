import { ClipboardType, FileText, Globe, Loader2, UploadCloud, X } from "lucide-react";
import { useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { kbDocumentAdd, kbDocumentAddUrl } from "../../api";
import { Button, Drawer, DrawerHeader } from "../../components/ui";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { joinClasses } from "../../components/ui/utils";
import { KB_DETAIL_COPY as C } from "./copy";

type Mode = "paste" | "file" | "url";

const FIELD =
  "w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition placeholder:text-foreground-muted focus:border-primary";

/** 添加资料抽屉：分段（粘贴文本 / 本地文件 / 网页）。成功后回调 onAdded 并关闭。 */
export function AddResourceDrawer({
  kbId,
  open,
  onClose,
  onAdded,
}: {
  kbId: string;
  open: boolean;
  onClose: () => void;
  onAdded: () => void;
}) {
  const notifications = useNotifications();
  const [mode, setMode] = useState<Mode>("paste");
  const [title, setTitle] = useState("");
  const [paste, setPaste] = useState("");
  const [url, setUrl] = useState("");
  const [filePath, setFilePath] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  function reset() {
    setMode("paste");
    setTitle("");
    setPaste("");
    setUrl("");
    setFilePath(null);
  }

  function close() {
    if (busy) return;
    reset();
    onClose();
  }

  const fileName = filePath ? filePath.split(/[\\/]/).pop() ?? "文件" : "";

  async function pickFile() {
    try {
      const picked = await openFileDialog({
        multiple: false,
        filters: [{ name: "资料", extensions: ["md", "txt", "markdown", "pdf", "docx", "pptx", "xlsx"] }],
      });
      if (picked && typeof picked === "string") setFilePath(picked);
    } catch (err) {
      notifications.error({ message: String(err) });
    }
  }

  const canSubmit =
    mode === "paste" ? paste.trim().length > 0 : mode === "file" ? !!filePath : url.trim().length > 0;

  async function submit() {
    if (!canSubmit) return;
    setBusy(true);
    try {
      if (mode === "paste") {
        await kbDocumentAdd(kbId, title.trim() || "未命名资料", { body: paste });
      } else if (mode === "file") {
        await kbDocumentAdd(kbId, fileName, { filePath: filePath! });
      } else {
        await kbDocumentAddUrl(kbId, title.trim() || url.trim(), url.trim());
      }
      notifications.success(C.added);
      reset();
      onAdded();
      onClose();
    } catch (err) {
      notifications.error({ title: C.addFailed, message: String(err) });
    } finally {
      setBusy(false);
    }
  }

  const tabs: { id: Mode; label: string; icon: typeof FileText }[] = [
    { id: "paste", label: C.tabPaste, icon: ClipboardType },
    { id: "file", label: C.tabFile, icon: FileText },
    { id: "url", label: C.tabUrl, icon: Globe },
  ];

  return (
    <Drawer open={open} onClose={close} title={C.addTitle} widthClassName="w-[min(560px,92vw)]">
      <DrawerHeader onClose={close}>
        <div className="min-w-0 flex gap-2 items-end">
          <h2 className="text-base font-semibold text-foreground">{C.addTitle}</h2>
          <p className="mt-0.5 text-xs text-foreground-muted">{C.addDesc}</p>
        </div>
      </DrawerHeader>

      <div className="flex min-h-0 flex-col">
        <div className="min-h-0 flex-1 space-y-3 overflow-auto px-5 py-4">
          {/* 分段控件 */}
          <div className="inline-flex rounded-lg border border-border-subtle bg-surface p-0.5">
            {tabs.map((t) => {
              const active = mode === t.id;
              const Icon = t.icon;
              return (
                <button
                  key={t.id}
                  type="button"
                  disabled={busy}
                  onClick={() => setMode(t.id)}
                  className={joinClasses(
                    "inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium transition",
                    active ? "bg-background text-foreground shadow-sm" : "text-foreground-muted hover:text-foreground"
                  )}
                >
                  <Icon className="h-3.5 w-3.5" />
                  {t.label}
                </button>
              );
            })}
          </div>

          {mode !== "file" ? (
            <input
              className={FIELD}
              placeholder={C.titlePlaceholder}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          ) : null}

          {mode === "paste" ? (
            <textarea
              autoFocus
              className={joinClasses(FIELD, "min-h-[220px] resize-y leading-6")}
              placeholder={C.pastePlaceholder}
              value={paste}
              onChange={(e) => setPaste(e.target.value)}
            />
          ) : null}

          {mode === "file" ? (
            filePath ? (
              <div className="flex items-center gap-3 rounded-lg border border-border-subtle bg-surface px-3 py-3">
                <div className="grid h-9 w-9 shrink-0 place-items-center rounded-md bg-muted">
                  <FileText className="h-4 w-4 text-foreground-muted" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium text-foreground">{fileName}</div>
                  <button
                    type="button"
                    onClick={() => void pickFile()}
                    className="text-xs text-primary transition hover:opacity-80"
                  >
                    {C.pickAnother}
                  </button>
                </div>
                <button
                  type="button"
                  aria-label="清除"
                  onClick={() => setFilePath(null)}
                  className="grid h-7 w-7 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
                >
                  <X className="h-4 w-4" />
                </button>
              </div>
            ) : (
              <button
                type="button"
                onClick={() => void pickFile()}
                className="flex w-full flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-border bg-background/60 px-4 py-10 text-center transition hover:border-primary hover:bg-background"
              >
                <UploadCloud className="h-7 w-7 text-foreground-muted" />
                <span className="text-sm font-medium text-foreground">{C.pickFile}</span>
                <span className="text-xs text-foreground-muted">{C.fileHint}</span>
              </button>
            )
          ) : null}

          {mode === "url" ? (
            <div>
              <input
                autoFocus
                className={FIELD}
                placeholder={C.urlPlaceholder}
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && canSubmit) void submit();
                }}
              />
              <p className="mt-1 text-xs text-foreground-muted">{C.urlHint}</p>
            </div>
          ) : null}
        </div>

        <div className="flex items-center justify-end gap-2 border-t border-border bg-surface px-5 py-3">
          <button
            type="button"
            onClick={close}
            disabled={busy}
            className="rounded-lg px-3 py-2 text-sm font-medium text-foreground-secondary transition hover:bg-accent hover:text-foreground disabled:opacity-50"
          >
            {C.cancel}
          </button>
          <Button tone="primary" onClick={() => void submit()} disabled={busy || !canSubmit}>
            {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
            {busy ? C.adding : mode === "url" ? C.urlAdd : C.addByPaste}
          </Button>
        </div>
      </div>
    </Drawer>
  );
}
