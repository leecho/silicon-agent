import { ChevronRight, Folder, FolderOpen, RefreshCw } from "lucide-react";
import { useState } from "react";
import type { Artifact } from "../../types";
import { Tooltip } from "../ui/Tooltip";
import { artifactFileName, artifactIcon } from "./artifactFilePresentation";
import { buildWorkspaceTree, type WorkspaceTreeNode } from "./workspaceTree";

// 工作空间 tab 正文：顶部工作目录行 + 刷新；主体为文件夹树；命中已登记产物的文件加徽章（区分 final/working）。
// files/loading/error 由父组件（SessionPage）拉取并透传，便于随 artifacts_updated 重拉。
export function WorkspaceTab({
  workspaceLabel,
  workspacePath,
  files,
  truncated,
  loading,
  error,
  artifacts,
  onOpenDir,
  onPreviewFile,
  onRefresh,
}: {
  workspaceLabel: string;
  workspacePath?: string;
  files: string[];
  truncated: boolean;
  loading: boolean;
  error?: string | null;
  artifacts: Artifact[];
  onOpenDir: () => void;
  onPreviewFile: (relPath: string) => void;
  onRefresh: () => void;
}) {
  const tree = buildWorkspaceTree(files);
  const artifactKind = new Map<string, string>();
  for (const a of artifacts) artifactKind.set(a.path, a.kind);

  return (
    <div className="flex h-full min-h-0 w-full flex-col gap-2 overflow-auto px-3 pb-3 pt-3 text-card-foreground">
      {/* 工作目录行 */}
      <div className="flex items-center justify-between gap-1 text-[12px] text-primary">
        <div className="flex min-w-0 items-center gap-1">
          <FolderOpen className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <Tooltip content={workspacePath} disabled={!workspacePath}>
            <button
              type="button"
              className="min-w-0 truncate rounded p-1 text-primary"
              disabled={!workspacePath}
              onClick={onOpenDir}
            >
              {workspaceLabel}
            </button>
          </Tooltip>
        </div>
        <Tooltip content="刷新">
          <button
            type="button"
            aria-label="刷新工作空间"
            className="grid h-6 w-6 shrink-0 place-items-center rounded text-foreground-secondary transition hover:bg-accent hover:text-foreground"
            onClick={onRefresh}
          >
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} aria-hidden="true" />
          </button>
        </Tooltip>
      </div>

      {/* 主体 */}
      {error ? (
        <div className="px-1 py-6 text-center text-[13px] text-foreground-muted">读取工作目录失败</div>
      ) : tree.length === 0 ? (
        <div className="px-1 py-6 text-center text-[13px] text-foreground-muted">
          {loading ? "加载中…" : "工作目录为空"}
        </div>
      ) : (
        <div className="flex flex-col">
          {tree.map((node) => (
            <TreeNode
              key={node.path}
              node={node}
              depth={0}
              artifactKind={artifactKind}
              onPreviewFile={onPreviewFile}
            />
          ))}
        </div>
      )}

      {truncated && (
        <div className="px-1 pt-1 text-[11px] text-foreground-muted">仅显示前 200 个文件</div>
      )}
    </div>
  );
}

function TreeNode({
  node,
  depth,
  artifactKind,
  onPreviewFile,
}: {
  node: WorkspaceTreeNode;
  depth: number;
  artifactKind: Map<string, string>;
  onPreviewFile: (relPath: string) => void;
}) {
  const [open, setOpen] = useState(true);
  const indent = { paddingLeft: `${depth * 12 + 4}px` };

  if (node.type === "dir") {
    return (
      <div className="flex flex-col">
        <button
          type="button"
          style={indent}
          className="flex items-center gap-1 rounded py-1 pr-2 text-left text-[13px] text-foreground-secondary hover:bg-accent"
          onClick={() => setOpen((v) => !v)}
        >
          <ChevronRight
            className={`h-3.5 w-3.5 shrink-0 transition ${open ? "rotate-90" : ""}`}
            aria-hidden="true"
          />
          {open ? (
            <FolderOpen className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
          ) : (
            <Folder className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
          )}
          <span className="min-w-0 flex-1 truncate">{node.name}</span>
        </button>
        {open &&
          node.children?.map((child) => (
            <TreeNode
              key={child.path}
              node={child}
              depth={depth + 1}
              artifactKind={artifactKind}
              onPreviewFile={onPreviewFile}
            />
          ))}
      </div>
    );
  }

  const Icon = artifactIcon(node.path);
  const kind = artifactKind.get(node.path);
  return (
    <Tooltip content={node.path}>
      <button
        type="button"
        style={indent}
        className="flex w-full items-center gap-1.5 rounded py-1 pr-2 text-left text-[13px] text-foreground-secondary hover:bg-accent"
        onClick={() => onPreviewFile(node.path)}
      >
        {/* 图标与目录 chevron 对齐：留出 3.5 宽占位 */}
        <span className="w-3.5 shrink-0" aria-hidden="true" />
        <Icon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
        <span className="min-w-0 flex-1 truncate">{artifactFileName(node.path)}</span>
        {kind && <ArtifactBadge kind={kind} />}
      </button>
    </Tooltip>
  );
}

// 产物徽章：final → 「最终」，working → 「工作」。
function ArtifactBadge({ kind }: { kind: string }) {
  const isWorking = kind === "working";
  return (
    <span
      className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${
        isWorking
          ? "bg-accent text-foreground-secondary"
          : "bg-primary/15 text-primary"
      }`}
    >
      {isWorking ? "工作" : "最终"}
    </span>
  );
}
