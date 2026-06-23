import { ChevronRight, CircleArrowOutUpRight, FolderOpen } from "lucide-react";
import type { Artifact } from "../../types";
import { artifactFileName, artifactIcon } from "./artifactFilePresentation";
import { useState, type MouseEvent } from "react";
import { Tooltip } from "../ui/Tooltip";

// 任务监控侧栏「产物」面板：顶部工作目录行（label + 系统打开）+「最终文件」产物列表。
// 始终渲染。label：默认目录显示「默认工作目录」，已选目录显示目录名（非全路径）；
// fullPath：实际目录绝对路径，用于 hover 提示与打开。
export function WorkspacePanel({
  label,
  fullPath,
  onOpen,
  artifacts,
  onOpenArtifact,
}: {
  label: string;
  fullPath?: string;
  onOpen: () => void;
  artifacts: Artifact[];
  onOpenArtifact: (a: Artifact) => void;
}) {
  const [open, setOpen] = useState(true);

  function handleOpenClick(event: MouseEvent<HTMLButtonElement>) {
    event.stopPropagation();
    onOpen();
  }

  return (
    <div className="shrink-0 flex flex-col gap-3">
      <div
        className="flex cursor-pointer items-center justify-between gap-2"
        onClick={() => setOpen((v) => !v)}
      >
        <div className="flex items-center gap-1.5 text-sm font-semibold text-foreground">
          产物
        </div>
        <span className="text-xs text-foreground-muted">
          <ChevronRight
            className={`h-3.5 w-3.5 shrink-0 transition ${open ? "rotate-90" : ""}`}
            aria-hidden="true"
          />
        </span>
      </div>
      {open && (
        <div className="flex items-center gap-1 text-[12px] text-primary">
          <FolderOpen className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <Tooltip content={fullPath} disabled={!fullPath}>
            <button
              type="button"
              className="shrink-0 rounded p-1 "
              disabled={!fullPath}
              onClick={handleOpenClick}
            >
            <span className="text-primary min-w-0 truncate ">{label}</span>
            </button>
          </Tooltip>
        </div>
      )}

      {open && (
        <>
          <ArtifactGroup
            label="最终文件"
            items={artifacts.filter((a) => a.kind !== "working")}
            onOpenArtifact={onOpenArtifact}
          />
          <ArtifactGroup
            label="工作文件"
            items={artifacts.filter((a) => a.kind === "working")}
            onOpenArtifact={onOpenArtifact}
          />
        </>
      )}
    </div>
  );
}

// 单组产物列表（「最终文件」/「工作文件」）。仅在非空时渲染。
function ArtifactGroup({
  label,
  items,
  onOpenArtifact,
}: {
  label: string;
  items: Artifact[];
  onOpenArtifact: (a: Artifact) => void;
}) {
  if (items.length === 0) return null;
  return (
    <div className="mt-1 flex flex-col gap-1">
      <div className="text-xs font-medium text-foreground-muted">{label}</div>
      {items.map((a) => {
        const Icon = artifactIcon(a.path);
        return (
          <Tooltip key={a.path} content={a.path}>
            <button
              type="button"
              className="flex w-full items-center gap-1.5 rounded px-2 py-1 text-left text-[13px] text-foreground-secondary hover:bg-accent"
              onClick={() => onOpenArtifact(a)}
            >
              <Icon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
              <span className="min-w-0 flex-1 truncate">
                {artifactFileName(a.path)}
              </span>
            </button>
          </Tooltip>
        );
      })}
    </div>
  );
}
