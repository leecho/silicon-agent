import { FileText, Loader2, Search } from "lucide-react";
import { useState } from "react";
import { kbSearch } from "../../api";
import type { KnowledgeHit } from "../../types";
import { Button, Drawer, DrawerHeader } from "../../components/ui";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { KB_DETAIL_COPY as C } from "./copy";

/** 查阅预览抽屉：按需打开，输入问题看资料库能查到的片段。 */
export function SearchDrawer({ kbId, open, onClose }: { kbId: string; open: boolean; onClose: () => void }) {
  const notifications = useNotifications();
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<KnowledgeHit[] | null>(null);
  const [searching, setSearching] = useState(false);

  async function run() {
    if (!query.trim()) return;
    setSearching(true);
    try {
      setHits(await kbSearch([kbId], query));
    } catch (err) {
      notifications.error({ message: String(err) });
    } finally {
      setSearching(false);
    }
  }

  return (
    <Drawer open={open} onClose={onClose} title={C.previewTitle} widthClassName="w-[min(600px,92vw)]">
      <DrawerHeader onClose={onClose}>
        <div className="min-w-0 flex gap-2 items-end">
          <h2 className="text-base font-semibold text-foreground">{C.previewTitle}</h2>
          <p className="mt-0.5 text-xs text-foreground-muted">{C.previewDesc}</p>
        </div>
      </DrawerHeader>

      <div className="flex min-h-0 flex-col">
        <div className="border-b border-border-subtle px-5 py-3">
          <div className="flex gap-2">
            <input
              autoFocus
              className="flex-1 rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition placeholder:text-foreground-muted focus:border-primary"
              placeholder={C.previewPlaceholder}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void run();
              }}
            />
            <Button tone="primary" onClick={() => void run()} disabled={searching || !query.trim()}>
              {searching ? <Loader2 className="h-4 w-4 animate-spin" /> : <Search className="h-4 w-4" />}
              {C.previewRun}
            </Button>
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-auto px-5 py-4">
          {hits === null ? (
            <p className="py-8 text-center text-sm text-foreground-muted">{C.previewInitial}</p>
          ) : hits.length === 0 ? (
            <p className="py-8 text-center text-sm text-foreground-muted">{C.previewEmpty}</p>
          ) : (
            <ul className="flex flex-col gap-2">
              {hits.map((h) => (
                <li key={h.chunkId} className="rounded-lg border border-border-subtle bg-surface p-3">
                  <div className="mb-1 flex items-center gap-1.5 text-xs text-foreground-muted">
                    <FileText className="h-3 w-3 shrink-0" />
                    <span className="truncate">
                      {h.docTitle}
                      {h.headingPath ? ` › ${h.headingPath}` : ""}
                    </span>
                  </div>
                  <div className="text-sm leading-6 text-foreground-secondary">{h.content}</div>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </Drawer>
  );
}
