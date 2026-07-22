import { BookMarked, Loader2, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { kbCreate, kbDelete } from "../../api";
import type { KnowledgeBase } from "../../types";
import { Button, EmptyState, Modal, ModalHeader } from "../../components/ui";
import { useMessages } from "../../components/ui/MessageProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { KB_COPY } from "./copy";

/** 资料库首页：卡片网格（对齐 ProjectList 布局）。 */
export function KnowledgeBaseList({
  items,
  loading,
  onOpen,
  onReload,
  onCreated,
}: {
  items: KnowledgeBase[];
  loading: boolean;
  onOpen: (id: string) => void;
  onReload: () => void;
  onCreated: (kb: KnowledgeBase) => void;
}) {
  const messages = useMessages();
  const notifications = useNotifications();
  const [createOpen, setCreateOpen] = useState(false);
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");

  async function handleCreate() {
    const name = newName.trim();
    if (!name) return;
    setCreating(true);
    try {
      const kb = await kbCreate(name);
      setNewName("");
      setCreateOpen(false);
      notifications.success(KB_COPY.created);
      onCreated(kb);
    } catch (err) {
      notifications.error({ title: KB_COPY.createFailed, message: String(err) });
    } finally {
      setCreating(false);
    }
  }

  async function handleDelete(kb: KnowledgeBase) {
    const ok = await messages.confirm({ ...KB_COPY.deleteConfirm(kb.name), tone: "warning", confirmText: "删除" });
    if (!ok) return;
    try {
      await kbDelete(kb.id);
      notifications.success(KB_COPY.deleted);
      onReload();
    } catch (err) {
      notifications.error({ title: KB_COPY.deleteFailed, message: String(err) });
    }
  }

  return (
    <div className="h-full overflow-auto p-6 text-sm">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-6 mt-4 flex items-start justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold text-foreground">{KB_COPY.pageTitle}</h1>
            <p className="mt-1 text-xs text-foreground-muted">{KB_COPY.pageSubtitle}</p>
          </div>
          <Button tone="primary" className="shrink-0" onClick={() => setCreateOpen(true)}>
            <Plus className="h-4 w-4" /> {KB_COPY.newButton}
          </Button>
        </div>

        {loading ? (
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {[0, 1, 2].map((i) => (
              <div key={i} className="h-28 animate-pulse rounded-xl border border-border-subtle bg-surface" />
            ))}
          </div>
        ) : items.length === 0 ? (
          <EmptyState
            icon={BookMarked}
            title={KB_COPY.empty.title}
            description={KB_COPY.empty.desc}
            action={
              <Button tone="primary" onClick={() => setCreateOpen(true)}>
                <Plus className="h-4 w-4" /> {KB_COPY.newButton}
              </Button>
            }
          />
        ) : (
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {items.map((kb) => (
              <button
                key={kb.id}
                type="button"
                onClick={() => onOpen(kb.id)}
                className="group flex flex-col rounded-xl border border-border-subtle bg-surface p-4 text-left transition hover:border-border"
              >
                <div className="flex items-start gap-2.5">
                  <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg border border-border bg-background text-primary">
                    <BookMarked className="h-4 w-4" aria-hidden="true" />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-semibold text-foreground">{kb.name}</span>
                    <span className="mt-0.5 block text-xs text-foreground-muted">
                      {kb.docCount} {KB_COPY.docCountUnit}
                    </span>
                  </span>
                  <span
                    role="button"
                    tabIndex={0}
                    aria-label="删除"
                    onClick={(e) => {
                      e.stopPropagation();
                      void handleDelete(kb);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.stopPropagation();
                        void handleDelete(kb);
                      }
                    }}
                    className="rounded-md px-1.5 py-1 text-foreground-muted opacity-0 transition hover:text-destructive group-hover:opacity-100"
                  >
                    <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
                  </span>
                </div>
                {kb.description ? (
                  <p className="mt-2 line-clamp-2 text-xs leading-5 text-foreground-secondary">{kb.description}</p>
                ) : null}
              </button>
            ))}
          </div>
        )}
      </div>

      <Modal open={createOpen} onClose={() => !creating && setCreateOpen(false)} title={KB_COPY.createTitle}>
        <ModalHeader onClose={() => !creating && setCreateOpen(false)}>
          <h2 className="text-base font-semibold text-foreground">{KB_COPY.createTitle}</h2>
          <p className="mt-0.5 text-xs text-foreground-muted">{KB_COPY.createDesc}</p>
        </ModalHeader>
        <div className="mt-4">
          <label className="mb-1 block text-xs font-medium text-foreground-secondary">{KB_COPY.nameLabel}</label>
          <input
            autoFocus
            className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition placeholder:text-foreground-muted focus:border-primary"
            placeholder={KB_COPY.namePlaceholder}
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void handleCreate();
            }}
          />
        </div>
        <div className="mt-5 flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={() => setCreateOpen(false)}
            disabled={creating}
            className="rounded-lg px-3 py-2 text-sm font-medium text-foreground-secondary transition hover:bg-accent hover:text-foreground disabled:opacity-50"
          >
            {KB_COPY.cancel}
          </button>
          <Button tone="primary" onClick={() => void handleCreate()} disabled={creating || !newName.trim()}>
            {creating ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
            {creating ? KB_COPY.creating : KB_COPY.create}
          </Button>
        </div>
      </Modal>
    </div>
  );
}
