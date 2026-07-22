import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  AlertTriangle,
  ExternalLink,
  Eye,
  FolderOpen,
  Loader2,
  MessagesSquare,
  PackageOpen,
  RefreshCw,
  Search,
  type LucideIcon,
} from "lucide-react";

import {
  listProjectArtifacts,
  openArtifactFile,
  revealArtifactFile,
} from "../../api";
import { ArtifactPreviewDrawer } from "../../components/session/ArtifactPreviewDrawer";
import {
  artifactFileName,
  artifactIcon,
} from "../../components/session/artifactFilePresentation";
import { Button } from "../../components/ui/Button";
import { Tooltip } from "../../components/ui/Tooltip";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { Artifact, Project, ProjectArtifact } from "../../types";
import { ProjectMemberAvatar } from "./ProjectMemberAvatar";

export function ProjectArtifactList({
  project,
  onOpenSession,
}: {
  project: Project;
  onOpenSession: (id: string) => void;
}) {
  const notifications = useNotifications();
  const [items, setItems] = useState<ProjectArtifact[]>([]);
  const [phase, setPhase] = useState<"loading" | "ready" | "error">("loading");
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [preview, setPreview] = useState<ProjectArtifact | null>(null);

  async function reload() {
    setPhase("loading");
    setError("");
    try {
      const next = await listProjectArtifacts(project.id);
      setItems(next);
      setPhase("ready");
    } catch (err) {
      setError(String(err));
      setPhase("error");
    }
  }

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project.id]);

  const filtered = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized) return items;
    return items.filter((item) =>
      [
        item.title,
        item.path,
        item.task,
        item.displayName,
        item.expertName,
      ]
        .filter(Boolean)
        .join(" ")
        .toLocaleLowerCase()
        .includes(normalized),
    );
  }, [items, query]);

  function toPreviewArtifact(item: ProjectArtifact): Artifact {
    return {
      createdAt: "",
      kind: "final",
      path: item.path,
      title: item.title,
    };
  }

  function notifyError(title: string, err: unknown) {
    notifications.error({
      title,
      message: err instanceof Error ? err.message : String(err),
    });
  }

  async function handleOpenFile(item: ProjectArtifact) {
    try {
      await openArtifactFile(item.sessionId, item.path);
    } catch (err) {
      notifyError("打开产物失败", err);
    }
  }

  async function handleRevealFile(item: ProjectArtifact) {
    try {
      await revealArtifactFile(item.sessionId, item.path);
    } catch (err) {
      notifyError("定位产物失败", err);
    }
  }

  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto flex max-w-[860px] flex-col gap-4">
        <div className="flex flex-col gap-3 border-b border-border-subtle pb-4 sm:flex-row sm:items-center sm:justify-between">
          <div className="min-w-0">
            <h3 className="text-sm font-semibold text-foreground">
              产物 {phase === "ready" ? filtered.length : items.length}
            </h3>
            <p className="mt-1 text-[12px] leading-5 text-foreground-muted">
              聚合项目成员在子任务中登记的交付文件。
            </p>
          </div>
          <div className="flex min-w-0 items-center gap-2">
            <label className="flex h-9 min-w-0 flex-1 items-center gap-2 rounded-lg border border-border bg-background px-3 text-[13px] text-foreground-secondary sm:w-[260px] sm:flex-none">
              <Search className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
              <input
                className="min-w-0 flex-1 bg-transparent outline-none placeholder:text-foreground-muted"
                placeholder="搜索产物、成员或任务"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </label>
            <Button tone="outline" onClick={() => void reload()}>
              <RefreshCw className="h-4 w-4" aria-hidden="true" />
              刷新
            </Button>
          </div>
        </div>

        {phase === "loading" && (
          <ProjectArtifactState
            icon={<Loader2 className="h-5 w-5 animate-spin" aria-hidden="true" />}
            title="正在加载产物"
            description="从项目会话和成员子任务中汇总已登记文件。"
          />
        )}

        {phase === "error" && (
          <ProjectArtifactState
            icon={<AlertTriangle className="h-5 w-5" aria-hidden="true" />}
            title="加载产物失败"
            description={error}
            action={
              <Button tone="outline" onClick={() => void reload()}>
                <RefreshCw className="h-4 w-4" aria-hidden="true" />
                重试
              </Button>
            }
          />
        )}

        {phase === "ready" && items.length === 0 && (
          <ProjectArtifactState
            icon={<PackageOpen className="h-5 w-5" aria-hidden="true" />}
            title="还没有产物"
            description="成员任务登记文件后，会出现在这里。"
          />
        )}

        {phase === "ready" && items.length > 0 && filtered.length === 0 && (
          <ProjectArtifactState
            icon={<Search className="h-5 w-5" aria-hidden="true" />}
            title="没有匹配结果"
            description="换一个关键词试试。"
          />
        )}

        {phase === "ready" && filtered.length > 0 && (
          <ul className="flex flex-col gap-2">
            {filtered.map((item) => (
              <ProjectArtifactRow
                key={`${item.sessionId}:${item.path}`}
                item={item}
                onOpenFile={() => void handleOpenFile(item)}
                onOpenSession={() => onOpenSession(item.sessionId)}
                onPreview={() => setPreview(item)}
                onRevealFile={() => void handleRevealFile(item)}
              />
            ))}
          </ul>
        )}
      </div>

      <ArtifactPreviewDrawer
        artifact={preview ? toPreviewArtifact(preview) : null}
        sessionId={preview?.sessionId ?? ""}
        onClose={() => setPreview(null)}
      />
    </div>
  );
}

function ProjectArtifactRow({
  item,
  onOpenFile,
  onOpenSession,
  onPreview,
  onRevealFile,
}: {
  item: ProjectArtifact;
  onOpenFile: () => void;
  onOpenSession: () => void;
  onPreview: () => void;
  onRevealFile: () => void;
}) {
  const fileName = item.title.trim() || artifactFileName(item.path);
  const Icon = artifactIcon(item.path);
  const memberLabel = item.displayName?.trim() || item.expertName;

  return (
    <li className="group flex min-w-0 items-center gap-3 rounded-lg border border-border-subtle bg-surface px-3 py-3 transition hover:border-border">
      <button
        type="button"
        className="flex flex-col min-w-0 flex-1 gap-2 text-left"
        onClick={onPreview}
      >
        <span className="flex gap-3">
        <span className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-card text-foreground-secondary">
          <Icon className="h-4 w-4" aria-hidden="true" />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[13px] font-semibold text-foreground">
            {fileName}
          </span>
          <Tooltip content={item.path}>
            <span className="mt-0.5 block truncate text-[12px] text-foreground-muted">
              {item.path}
            </span>
          </Tooltip>
       </span>
        </span>
          {/* <span className="mt-1 flex min-w-0 items-center gap-2 text-[12px] text-foreground-secondary">
          <ProjectMemberAvatar
            member={{
              expertName: item.expertName,
              avatar: null,
              displayName: item.displayName,
              id: item.sessionId,
              isCoordinator: false,
              projectId: "",
              roleLabel: null,
              sort: 0,
            }}
          />
          <span className="shrink-0 truncate">{memberLabel}</span>
          <span className="shrink-0 text-foreground-muted">/</span>
          <span className="min-w-0 truncate">{item.title}</span>
        </span> */}
      </button>
      <div className="flex shrink-0 items-center gap-1">
        <ArtifactIconButton icon={Eye} label="预览" onClick={onPreview} />
        <ArtifactIconButton icon={ExternalLink} label="打开" onClick={onOpenFile} />
        <ArtifactIconButton icon={FolderOpen} label="在文件夹中显示" onClick={onRevealFile} />
        <ArtifactIconButton icon={MessagesSquare} label="打开来源会话" onClick={onOpenSession} />
      </div>
    </li>
  );
}

function ArtifactIconButton({
  icon: Icon,
  label,
  onClick,
}: {
  icon: LucideIcon;
  label: string;
  onClick: () => void;
}) {
  return (
    <Tooltip content={label}>
      <button
        type="button"
        aria-label={label}
        className="grid h-8 w-8 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
        onClick={onClick}
      >
        <Icon className="h-3.5 w-3.5" aria-hidden="true" />
      </button>
    </Tooltip>
  );
}

function ProjectArtifactState({
  action,
  description,
  icon,
  title,
}: {
  action?: ReactNode;
  description: string;
  icon: ReactNode;
  title: string;
}) {
  return (
    <div className="grid min-h-[260px] place-items-center rounded-lg border border-dashed border-border bg-surface px-5 py-12 text-center">
      <div className="flex max-w-[360px] flex-col items-center gap-3">
        <span className="grid h-11 w-11 place-items-center rounded-lg bg-background text-foreground-muted">
          {icon}
        </span>
        <div>
          <div className="text-sm font-semibold text-foreground">{title}</div>
          <p className="mt-1 text-[12px] leading-5 text-foreground-muted">
            {description}
          </p>
        </div>
        {action}
      </div>
    </div>
  );
}
