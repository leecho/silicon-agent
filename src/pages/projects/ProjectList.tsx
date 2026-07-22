import { useState } from "react";
import { FolderKanban, Plus, Trash2 } from "lucide-react";

import { deleteProject } from "../../api";
import { Button } from "../../components/ui/Button";
import { useMessages } from "../../components/ui/MessageProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { Project } from "../../types";
import { EmptyState } from "./EmptyState";
import { NewProjectModal } from "./NewProjectModal";

export function ProjectList({
  projects,
  loading,
  onOpenProject,
  onCreated,
  onReload,
}: {
  projects: Project[];
  loading: boolean;
  onOpenProject: (id: string) => void;
  onCreated: (project: Project) => void;
  onReload: () => void;
}) {
  const messages = useMessages();
  const notify = useNotifications();

  async function removeProject(project: Project) {
    const ok = await messages.confirm({
      title: "删除项目",
      message: `确定删除项目「${project.name}」吗？该操作会删除项目记录及相关配置。`,
      tone: "warning",
      confirmText: "删除",
    });
    if (!ok) return;
    try {
      await deleteProject(project.id);
      onReload();
    } catch (err) {
      notify.notify({ tone: "error", title: "删除项目失败", message: String(err) });
    }
  }

  return (
    <div className="h-full overflow-auto p-6 text-sm">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-6 mt-4 flex items-start justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold text-foreground">项目</h1>
            <p className="mt-1 text-xs text-foreground-muted">
              把专家拉进一个项目，像群聊一样协作——项目经理(PM)自行决定直接回答、路由给成员、或拆解派发任务。
            </p>
          </div>
          <NewProjectButton onCreated={onCreated} />
        </div>

        {loading ? (
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {[0, 1, 2].map((i) => (
              <div key={i} className="h-28 animate-pulse rounded-xl border border-border-subtle bg-surface" />
            ))}
          </div>
        ) : projects.length === 0 ? (
          <EmptyState icon={<FolderKanban className="h-6 w-6" aria-hidden="true" />} title="还没有项目" hint="新建一个项目，拉入几个专家成员，就能开始多专家协作。" />
        ) : (
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {projects.map((p) => (
              <button key={p.id} type="button" onClick={() => onOpenProject(p.id)} className="group flex min-h-[112px] flex-col rounded-xl border border-border-subtle bg-surface p-4 text-left transition hover:border-border">
                <div className="flex items-start gap-2.5">
                  <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg border border-border bg-background text-primary">
                    <FolderKanban className="h-4 w-4" aria-hidden="true" />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-semibold text-foreground">{p.name}</span>
                  </span>
                  <span
                    role="button"
                    tabIndex={0}
                    onClick={(e) => {
                      e.stopPropagation();
                      void removeProject(p);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.stopPropagation();
                        void removeProject(p);
                      }
                    }}
                    className="rounded-md px-1.5 py-1 text-foreground-muted opacity-0 transition hover:text-destructive group-hover:opacity-100"
                  >
                    <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
                  </span>
                </div>
                {p.description && <p className="mt-2 line-clamp-2 text-xs leading-5 text-foreground-secondary">{p.description}</p>}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function NewProjectButton({ onCreated }: { onCreated: (p: Project) => void }) {
  const notify = useNotifications();
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button tone="primary" onClick={() => setOpen(true)}>
        <Plus className="h-4 w-4" aria-hidden="true" /> 新建项目
      </Button>
      {open && (
        <NewProjectModal
          onClose={() => setOpen(false)}
          onCreated={(p) => { setOpen(false); onCreated(p); }}
          notifyErr={(msg) => notify.notify({ tone: "error", title: "新建失败", message: msg })}
        />
      )}
    </>
  );
}
