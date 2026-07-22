import type { ReactNode } from "react";
import { useMemo, useState } from "react";
import { Eye, FolderOpen, MessagesSquare, PackageOpen, RefreshCw, Search } from "lucide-react";
import { openArtifactFile, revealArtifactFile } from "../../api";
import { ArtifactPreviewDrawer } from "../../components/session/ArtifactPreviewDrawer";
import { artifactFileName, artifactIcon } from "../../components/session/artifactFilePresentation";
import { Button } from "../../components/ui/Button";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { Agent, Artifact, ProjectArtifact } from "../../types";

export function AgentArtifactList({
  agent,
  artifacts,
  onOpenSession,
  onReload,
}: {
  agent: Agent;
  artifacts: ProjectArtifact[];
  onOpenSession: (id: string) => void;
  onReload: () => void;
}) {
  const notify = useNotifications();
  const [query, setQuery] = useState("");
  const [preview, setPreview] = useState<ProjectArtifact | null>(null);
  const filtered = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized) return artifacts;
    return artifacts.filter((item) => [item.title, item.path, item.task].join(" ").toLocaleLowerCase().includes(normalized));
  }, [artifacts, query]);

  function toPreviewArtifact(item: ProjectArtifact): Artifact {
    return { createdAt: "", kind: "final", path: item.path, title: item.title };
  }

  async function handleOpenFile(item: ProjectArtifact) {
    await openArtifactFile(item.sessionId, item.path).catch((err) => notify.notify({ tone: "error", title: "打开产物失败", message: String(err) }));
  }

  async function handleRevealFile(item: ProjectArtifact) {
    await revealArtifactFile(item.sessionId, item.path).catch((err) => notify.notify({ tone: "error", title: "定位产物失败", message: String(err) }));
  }

  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto flex max-w-[860px] flex-col gap-4">
        <div className="flex flex-col gap-3 border-b border-border-subtle pb-4 sm:flex-row sm:items-center sm:justify-between">
          <div className="min-w-0">
            <h3 className="text-sm font-semibold text-foreground">产物 {filtered.length}</h3>
            <p className="mt-1 text-[12px] leading-5 text-foreground-muted">聚合「{agent.displayName || agent.name}」会话登记的交付文件。</p>
          </div>
          <div className="flex min-w-0 items-center gap-2">
            <label className="flex h-9 min-w-0 flex-1 items-center gap-2 rounded-lg border border-border bg-background px-3 text-[13px] text-foreground-secondary sm:w-[260px] sm:flex-none">
              <Search className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
              <input className="min-w-0 flex-1 bg-transparent outline-none placeholder:text-foreground-muted" placeholder="搜索产物或任务" value={query} onChange={(event) => setQuery(event.target.value)} />
            </label>
            <Button tone="outline" onClick={onReload}><RefreshCw className="h-4 w-4" aria-hidden="true" />刷新</Button>
          </div>
        </div>

        {artifacts.length === 0 ? (
          <ArtifactState icon={<PackageOpen className="h-5 w-5" aria-hidden="true" />} title="还没有产物" description="智能体登记文件后，会出现在这里。" />
        ) : filtered.length === 0 ? (
          <ArtifactState icon={<Search className="h-5 w-5" aria-hidden="true" />} title="没有匹配结果" description="换一个关键词试试。" />
        ) : (
          <ul className="flex flex-col gap-2">
            {filtered.map((item) => {
              const Icon = artifactIcon(item.path);
              const fileName = item.title.trim() || artifactFileName(item.path);
              return (
                <li key={`${item.sessionId}:${item.path}`} className="flex items-center gap-3 rounded-xl border border-border-subtle bg-surface px-3 py-3 transition hover:border-border">
                  <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg border border-border-subtle bg-background text-foreground-secondary">
                    <Icon className="h-4 w-4" aria-hidden="true" />
                  </span>
                  <button type="button" onClick={() => setPreview(item)} className="min-w-0 flex-1 text-left">
                    <span className="block truncate text-[13px] font-medium text-foreground">{fileName}</span>
                    <span className="mt-0.5 block truncate text-[11px] text-foreground-muted">{item.task || item.path}</span>
                  </button>
                  <button type="button" title="预览" onClick={() => setPreview(item)} className="rounded px-1 py-1 text-foreground-muted transition hover:text-foreground"><Eye className="h-3.5 w-3.5" aria-hidden="true" /></button>
                  <button type="button" title="打开文件" onClick={() => void handleOpenFile(item)} className="rounded px-1 py-1 text-foreground-muted transition hover:text-foreground"><FolderOpen className="h-3.5 w-3.5" aria-hidden="true" /></button>
                  <button type="button" title="打开会话" onClick={() => onOpenSession(item.sessionId)} className="rounded px-1 py-1 text-foreground-muted transition hover:text-foreground"><MessagesSquare className="h-3.5 w-3.5" aria-hidden="true" /></button>
                  <button type="button" title="定位文件" onClick={() => void handleRevealFile(item)} className="rounded px-1 py-1 text-foreground-muted transition hover:text-foreground"><FolderOpen className="h-3.5 w-3.5" aria-hidden="true" /></button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
      <ArtifactPreviewDrawer artifact={preview ? toPreviewArtifact(preview) : null} sessionId={preview?.sessionId ?? ""} onClose={() => setPreview(null)} />
    </div>
  );
}

function ArtifactState({ description, icon, title }: { description: string; icon: ReactNode; title: string }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-border px-4 py-12 text-center">
      <span className="text-foreground-muted">{icon}</span>
      <p className="text-sm font-medium text-foreground">{title}</p>
      <p className="text-xs text-foreground-muted">{description}</p>
    </div>
  );
}
