import { useEffect, useState } from "react";
import { kbDocumentPreview } from "../../api";
import type { ArtifactContent } from "../../api";
import type { KnowledgeDocument } from "../../types";
import { Badge, Drawer, DrawerHeader } from "../../components/ui";
import { useNotifications } from "../../components/ui/NotificationProvider";
import {
  ArtifactContentView,
  PreviewErrorCard,
  PreviewLoading,
} from "../../components/session/ArtifactContentView";
import { KB_DETAIL_COPY as C } from "./copy";

/** 查看一份资料：有原始文件走富预览（md/pdf/office…），否则回退已存文本。 */
export function DocumentViewerDrawer({ doc, onClose }: { doc: KnowledgeDocument | null; onClose: () => void }) {
  const notifications = useNotifications();
  const [state, setState] = useState<
    | { phase: "loading" }
    | { phase: "error"; message: string }
    | { phase: "ready"; content: ArtifactContent }
  >({ phase: "loading" });

  useEffect(() => {
    if (!doc) return;
    setState({ phase: "loading" });
    let cancelled = false;
    kbDocumentPreview(doc.id)
      .then((content) => {
        if (!cancelled) setState({ phase: "ready", content });
      })
      .catch((err) => {
        if (!cancelled) {
          setState({ phase: "error", message: String(err) });
          notifications.error({ message: String(err) });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [doc, notifications]);

  return (
    <Drawer open={!!doc} onClose={onClose} title={doc?.title} widthClassName="w-[min(1040px,94vw)]">
      <DrawerHeader onClose={onClose}>
        <div className="min-w-0 flex gap-2 items-end">
          <h2 className="truncate text-base font-semibold text-foreground">{doc?.title}</h2>
          <div className="mt-1 flex items-center gap-2 text-xs text-foreground-muted">
            <Badge tone={doc?.status === "ready" ? "success" : doc?.status === "error" ? "danger" : "warning"}>
              {doc ? C.statusText(doc.status) : ""}
            </Badge>
            <span>
              {doc?.charSize ?? 0} {C.charUnit}
            </span>
          </div>
        </div>
      </DrawerHeader>

      <div className="min-h-0 flex-1 overflow-auto px-5 py-4">
        {doc?.status === "error" ? (
          <p className="rounded-md border border-danger-border bg-danger-subtle px-3 py-2 text-sm text-danger">
            {doc.error || C.viewEmpty}
          </p>
        ) : state.phase === "loading" ? (
          <PreviewLoading label={C.viewLoading} />
        ) : state.phase === "error" ? (
          <PreviewErrorCard message={state.message} />
        ) : (state.content.kind === "text" || state.content.kind === "markdown") &&
          state.content.content.trim() === "" ? (
          <p className="py-8 text-center text-sm text-foreground-muted">{C.viewEmpty}</p>
        ) : (
          <ArtifactContentView content={state.content} fileName={doc?.title ?? ""} />
        )}
      </div>
    </Drawer>
  );
}
