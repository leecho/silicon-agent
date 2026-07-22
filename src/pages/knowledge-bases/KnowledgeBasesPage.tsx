import { useEffect, useState } from "react";
import { kbList } from "../../api";
import type { KnowledgeBase } from "../../types";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { KnowledgeBaseList } from "./KnowledgeBaseList";
import { KnowledgeBaseDetail } from "./KnowledgeBaseDetail";
import { KB_COPY } from "./copy";

/** 资料库页：顶层壳，负责列表加载与「列表 / 详情」切换（对齐 ProjectsPage）。 */
export function KnowledgeBasesPage({
  knowledgeBaseId,
  onBack,
  onOpenKnowledgeBase,
  onOpenList,
}: {
  knowledgeBaseId?: string | null;
  onBack: () => void;
  onOpenKnowledgeBase: (id: string) => void;
  onOpenList: () => void;
}) {
  const notifications = useNotifications();
  const [items, setItems] = useState<KnowledgeBase[]>([]);
  const [loading, setLoading] = useState(true);

  async function reload() {
    try {
      setItems(await kbList());
    } catch (err) {
      notifications.error({ title: KB_COPY.loadFailed, message: String(err) });
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void reload();
  }, []);

  const open = items.find((k) => k.id === knowledgeBaseId) ?? null;
  if (open) {
    return (
      <KnowledgeBaseDetail
        kb={open}
        onBack={() => {
          onBack();
          void reload();
        }}
      />
    );
  }

  return (
    <KnowledgeBaseList
      items={items}
      loading={loading}
      onOpen={onOpenKnowledgeBase}
      onReload={() => {
        onOpenList();
        void reload();
      }}
      onCreated={(kb) => {
        void reload();
        onOpenKnowledgeBase(kb.id);
      }}
    />
  );
}
