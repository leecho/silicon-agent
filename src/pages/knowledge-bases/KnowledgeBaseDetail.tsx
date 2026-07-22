import { ArrowLeft, FileText, Loader2, Plus, Search, Sparkles, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { kbBuildVectorIndex, kbDocumentDelete, kbDocumentList, kbVectorSettings } from "../../api";
import type { KnowledgeBase, KnowledgeDocument } from "../../types";
import { Badge, Button, EmptyState } from "../../components/ui";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { useMessages } from "../../components/ui/MessageProvider";
import { AddResourceDrawer } from "./AddResourceDrawer";
import { DocumentViewerDrawer } from "./DocumentViewerDrawer";
import { SearchDrawer } from "./SearchDrawer";
import { KB_DETAIL_COPY as C } from "./copy";

export function KnowledgeBaseDetail({ kb, onBack }: { kb: KnowledgeBase; onBack: () => void }) {
  const notifications = useNotifications();
  const messages = useMessages();
  const [docs, setDocs] = useState<KnowledgeDocument[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [viewing, setViewing] = useState<KnowledgeDocument | null>(null);
  const [vectorOn, setVectorOn] = useState(false);
  const [building, setBuilding] = useState(false);

  async function reload() {
    setLoading(true);
    setError(null);
    try {
      setDocs(await kbDocumentList(kb.id));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }
  useEffect(() => {
    void reload();
  }, [kb.id]);

  useEffect(() => {
    void (async () => {
      try {
        const [on] = await kbVectorSettings();
        setVectorOn(on);
      } catch {
        /* 忽略：拿不到设置就不显示按钮 */
      }
    })();
  }, []);

  async function buildIndex() {
    setBuilding(true);
    try {
      const n = await kbBuildVectorIndex(kb.id);
      notifications.success(C.buildIndexDone(n));
    } catch (err) {
      notifications.error({ title: C.buildIndexFailed, message: String(err) });
    } finally {
      setBuilding(false);
    }
  }

  async function delDoc(e: React.MouseEvent, d: KnowledgeDocument) {
    e.stopPropagation();
    const ok = await messages.confirm({ ...C.deleteDocConfirm(d.title), tone: "warning", confirmText: "删除" });
    if (!ok) return;
    const prev = docs;
    setDocs((p) => p.filter((x) => x.id !== d.id));
    try {
      await kbDocumentDelete(d.id);
      notifications.success(C.docDeleted);
    } catch (err) {
      setDocs(prev);
      notifications.error({ message: String(err) });
    }
  }

  return (
    <div className="flex h-full flex-col text-sm">
      {/* 一级导航：返回 + 库名（对齐智能体详情标题栏） */}
      <div className="session-header flex items-center gap-2 border-b border-border-subtle px-4 pt-2.5 pb-1.5">
        <button
          type="button"
          onClick={onBack}
          aria-label="返回"
          className="grid h-8 w-8 shrink-0 place-items-center rounded-md text-foreground-secondary transition hover:bg-accent hover:text-accent-foreground"
        >
          <ArrowLeft className="h-4 w-4" aria-hidden="true" />
        </button>
        <span className="min-w-0 flex-1 truncate text-[15px] font-semibold text-foreground">{kb.name}</span>
      </div>

      {/* 内容区 */}
      <div className="min-h-0 flex-1 overflow-auto p-6">
        <div className="mx-auto max-w-[860px]">
          <div className="ml-auto flex shrink-0 items-center gap-2 px-5 py-2 justify-end">
          {vectorOn ? (
            <Button tone="outline" onClick={() => void buildIndex()} disabled={building}>
              {building ? <Loader2 className="h-4 w-4 animate-spin" /> : <Sparkles className="h-4 w-4" />}
              {building ? C.building : C.buildIndex}
            </Button>
          ) : null}
          <Button tone="outline" onClick={() => setSearchOpen(true)}>
            <Search className="h-4 w-4" /> {C.preview}
          </Button>
          <Button tone="primary" onClick={() => setAddOpen(true)}>
            <Plus className="h-4 w-4" /> {C.addResource}
          </Button>
        </div>
          {kb.description ? <p className="mb-4 text-xs text-foreground-muted">{kb.description}</p> : null}

          <div className="mb-2 flex items-center gap-2">
            <span className="text-xs font-medium uppercase tracking-wide text-foreground-muted">{C.docsTitle}</span>
            {!loading && !error && docs.length > 0 ? (
              <span className="text-xs text-foreground-muted">· {docs.length}</span>
            ) : null}
          </div>

          {loading ? (
            <div className="flex flex-col gap-2">
              {[0, 1, 2].map((i) => (
                <div key={i} className="h-[58px] animate-pulse rounded-lg border border-border-subtle bg-surface" />
              ))}
            </div>
          ) : error ? (
            <EmptyState
              icon={FileText}
              title={C.loadDocsFailed}
              description={error}
              action={
                <Button tone="outline" onClick={() => void reload()}>
                  {C.retry}
                </Button>
              }
            />
          ) : docs.length === 0 ? (
            <EmptyState
              icon={FileText}
              title={C.emptyDocs.title}
              description={C.emptyDocs.desc}
              action={
                <Button tone="primary" onClick={() => setAddOpen(true)}>
                  <Plus className="h-4 w-4" /> {C.addResource}
                </Button>
              }
            />
          ) : (
            <ul className="flex flex-col gap-2">
              {docs.map((d) => (
                <li
                  key={d.id}
                  className="group flex cursor-pointer items-center gap-3 rounded-lg border border-border-subtle bg-surface px-4 py-2.5 transition hover:border-border"
                  onClick={() => setViewing(d)}
                >
                  <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md bg-muted">
                    <FileText className="h-4 w-4 text-foreground-muted" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm text-foreground">{d.title}</div>
                    <div className="text-xs text-foreground-muted">
                      {d.charSize} {C.charUnit}
                    </div>
                  </div>
                  <Badge tone={d.status === "ready" ? "success" : d.status === "error" ? "danger" : "warning"}>
                    {C.statusText(d.status)}
                  </Badge>
                  <button
                    aria-label="删除"
                    className="grid h-7 w-7 shrink-0 place-items-center rounded-md text-foreground-muted opacity-0 transition hover:bg-accent hover:text-destructive group-hover:opacity-100"
                    onClick={(e) => void delDoc(e, d)}
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>

      <AddResourceDrawer kbId={kb.id} open={addOpen} onClose={() => setAddOpen(false)} onAdded={() => void reload()} />
      <SearchDrawer kbId={kb.id} open={searchOpen} onClose={() => setSearchOpen(false)} />
      <DocumentViewerDrawer doc={viewing} onClose={() => setViewing(null)} />
    </div>
  );
}
