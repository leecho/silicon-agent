import { useEffect, useState } from "react";
import { ArrowLeft, ChevronDown, ChevronRight, FileCode, FileImage, FileText, Folder, FolderOpen, Loader2, Wrench } from "lucide-react";
import { getSkillDetail, readSkillFile } from "../../api";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import { Drawer, DrawerHeader } from "../../components/ui/Drawer";
import { MarkdownText } from "../../components/ui/MarkdownText";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { SkillDetail, SkillFile, SkillFilePreview } from "../../types";
import { sortSkillFiles } from "./skillFilePreview";

type DetailPage = "default" | "files";

interface SkillFileTreeNode {
  name: string;
  relPath: string;
  type: "dir" | "file";
  children: SkillFileTreeNode[];
}

/** 技能详情抽屉：默认页渲染描述与技能正文，文件页浏览/预览目录文件。 */
export function SkillDetailDrawer({
  skillId,
  onClose,
}: {
  skillId: string | null;
  onClose: () => void;
}) {
  const notifications = useNotifications();
  const [detail, setDetail] = useState<SkillDetail | null>(null);
  const [page, setPage] = useState<DetailPage>("default");
  const [activeFile, setActiveFile] = useState<string | null>(null);
  const [preview, setPreview] = useState<SkillFilePreview | null>(null);
  const [collapsedDirs, setCollapsedDirs] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (!skillId) {
      setDetail(null);
      setPage("default");
      setActiveFile(null);
      setPreview(null);
      setCollapsedDirs(new Set());
      return;
    }
    setPage("default");
    setActiveFile(null);
    setPreview(null);
    setCollapsedDirs(new Set());
    getSkillDetail(skillId)
      .then((nextDetail) => {
        setDetail(nextDetail);
        setCollapsedDirs(new Set(collectSkillFileDirs(buildSkillFileTree(nextDetail.files))));
      })
      .catch((err) =>
        notifications.notify({ tone: "error", title: "加载详情失败", message: String(err) }),
      );
  }, [skillId, notifications]);

  async function openFile(relPath: string) {
    if (!skillId) return;
    setActiveFile(relPath);
    setPreview(null);
    try {
      setPreview(await readSkillFile(skillId, relPath));
    } catch (err) {
      notifications.notify({ tone: "error", title: "预览失败", message: String(err) });
    }
  }

  const files = detail ? sortSkillFiles(detail.files.filter((f) => !f.isDir)) : [];
  const fileTree = detail ? buildSkillFileTree(detail.files) : [];
  const displaySkillMd = detail ? stripSkillFrontmatterForDisplay(detail.skillMd) : "";

  function resetCollapsedDirs() {
    setCollapsedDirs(new Set(collectSkillFileDirs(fileTree)));
  }

  function toggleDir(relPath: string) {
    setCollapsedDirs((prev) => {
      const next = new Set(prev);
      if (next.has(relPath)) {
        next.delete(relPath);
      } else {
        next.add(relPath);
      }
      return next;
    });
  }

  return (
    <Drawer
      className="bg-popover text-popover-foreground"
      open={skillId !== null}
      onClose={onClose}
      width="640px"
      title={detail?.skill.name}
    >
      <DrawerHeader onClose={onClose}>
        <div className="flex min-w-0 items-center justify-between gap-4">
          <div className="flex min-w-0 items-center gap-3">
            <div className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-card text-foreground-secondary">
              <Wrench className="h-5 w-5" aria-hidden="true" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex min-w-0 items-center gap-2">
                <h2 className="truncate text-base font-semibold text-foreground">
                  {detail?.skill.name ?? "技能详情"}
                </h2>
                {detail && (
                  <>
                    <Badge tone={detail.skill.source === "builtin" ? "info" : "neutral"}>
                      {detail.skill.source === "builtin" ? "内置" : "用户安装"}
                    </Badge>
                    <Badge tone={detail.skill.enabled ? "success" : "neutral"}>
                      {detail.skill.enabled ? "已启用" : "已禁用"}
                    </Badge>
                  </>
                )}
              </div>
            </div>
          </div>
          {detail && page === "default" && (
            <Button
              tone="outline"
              className="shrink-0 px-3 py-1.5 text-xs"
              onClick={() => {
                resetCollapsedDirs();
                setPage("files");
              }}
            >
              <FolderOpen className="h-3.5 w-3.5" aria-hidden="true" />
              查看文件
            </Button>
          )}
          {detail && page === "files" && (
            <Button
              tone="outline"
              className="shrink-0 px-3 py-1.5 text-xs"
              onClick={() => {
                setPage("default");
                setActiveFile(null);
                setPreview(null);
              }}
            >
              <ArrowLeft className="h-3.5 w-3.5" aria-hidden="true" />
              返回详情
            </Button>
          )}
        </div>
      </DrawerHeader>

      <div className="grid min-h-0 grid-rows-[minmax(0,1fr)] bg-surface">
        <div className={`min-h-0 px-5 py-4 ${page === "files" ? "overflow-hidden" : "overflow-y-auto"}`}>
          {!detail ? (
            <div className="grid h-full place-items-center text-sm text-foreground-muted">
              <div className="flex items-center gap-2">
                <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
                加载中...
              </div>
            </div>
          ) : page === "default" ? (
            <div className="mx-auto bg-surface">
              <div className="mb-4 border-b border-border-subtle pb-4">
                <div className="min-w-0">
                  <h3 className="text-sm font-semibold text-foreground">技能说明</h3>
                  {detail.skill.description ? (
                    <p className="mt-2 text-sm leading-6 text-foreground-secondary">
                      {detail.skill.description}
                    </p>
                  ) : (
                    <p className="mt-2 text-sm text-foreground-muted">暂无描述</p>
                  )}
                </div>
              </div>
              <div className="px-1 py-1">
                <MarkdownText
                  value={displaySkillMd}
                  className="max-w-full [overflow-wrap:anywhere]"
                />
              </div>
            </div>
          ) : (
            <div className="flex h-full min-h-0 flex-col">
              <div className="shrink-0 border-b border-border-subtle pb-3">
                <div className="flex flex-row gap-2">
                  <h3 className="text-sm font-semibold text-foreground">技能文件</h3>
                  <p className="mt-1 text-xs text-foreground-muted">
                    共 {files.length} 个文件
                  </p>
                </div>
              </div>
              <div className="grid min-h-0 flex-1 grid-cols-[200px_minmax(0,1fr)] gap-4 pt-4">
                <aside className="flex min-h-0 flex-col rounded-lg border border-border-subtle bg-surface p-2">
                  {files.length === 0 ? (
                    <div className="px-2 py-3 text-sm text-foreground-muted">暂无文件</div>
                  ) : (
                    <ul className="min-h-0 flex-1 space-y-0.5 overflow-x-auto overflow-y-auto">
                      {fileTree.map((node) => (
                        <SkillFileTreeItem
                          key={node.relPath}
                          node={node}
                          activeFile={activeFile}
                          collapsedDirs={collapsedDirs}
                          depth={0}
                          onOpenFile={openFile}
                          onToggleDir={toggleDir}
                        />
                      ))}
                    </ul>
                  )}
                </aside>
                <div className="min-h-0 min-w-0 overflow-hidden rounded-lg border border-border-subtle bg-surface">
                  {!activeFile ? (
                    <EmptyPreview />
                  ) : !preview ? (
                    <div className="grid min-h-[260px] place-items-center text-sm text-foreground-muted">
                      <div className="flex items-center gap-2">
                        <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
                        加载中...
                      </div>
                    </div>
                  ) : (
                    <div className="h-full min-h-0 overflow-auto">
                      <FilePreviewBody preview={preview} />
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </Drawer>
  );
}

function stripSkillFrontmatterForDisplay(markdown: string) {
  const normalized = markdown.replace(/^\uFEFF/, "");
  const lines = normalized.split(/\r?\n/);
  if (lines[0]?.trim() !== "---") return markdown;
  const endIndex = lines.findIndex((line, index) => index > 0 && line.trim() === "---");
  if (endIndex === -1) return markdown;
  return lines.slice(endIndex + 1).join("\n").replace(/^\s+/, "");
}

function buildSkillFileTree(entries: SkillFile[]): SkillFileTreeNode[] {
  const root: SkillFileTreeNode = { name: "", relPath: "", type: "dir", children: [] };
  const dirs = new Map<string, SkillFileTreeNode>([["", root]]);

  const ensureDir = (relPath: string) => {
    const normalized = relPath.replace(/^\/+|\/+$/g, "");
    if (!normalized) return root;
    const existing = dirs.get(normalized);
    if (existing) return existing;

    const parts = normalized.split("/");
    const name = parts[parts.length - 1] ?? normalized;
    const parentPath = parts.slice(0, -1).join("/");
    const parent = ensureDir(parentPath);
    const node: SkillFileTreeNode = { name, relPath: normalized, type: "dir", children: [] };
    parent.children.push(node);
    dirs.set(normalized, node);
    return node;
  };

  for (const entry of sortSkillFiles(entries)) {
    const normalized = entry.relPath.replace(/^\/+|\/+$/g, "");
    if (!normalized) continue;
    if (entry.isDir) {
      ensureDir(normalized);
      continue;
    }

    const parts = normalized.split("/");
    const name = parts[parts.length - 1] ?? normalized;
    const parent = ensureDir(parts.slice(0, -1).join("/"));
    if (!parent.children.some((child) => child.type === "file" && child.relPath === normalized)) {
      parent.children.push({ name, relPath: normalized, type: "file", children: [] });
    }
  }

  const sortNodes = (nodes: SkillFileTreeNode[]) => {
    nodes.sort((a, b) => {
      if (a.relPath === "SKILL.md") return -1;
      if (b.relPath === "SKILL.md") return 1;
      if (a.type !== b.type) return a.type === "dir" ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    for (const node of nodes) sortNodes(node.children);
  };

  sortNodes(root.children);
  return root.children;
}

function collectSkillFileDirs(nodes: SkillFileTreeNode[]): string[] {
  const dirs: string[] = [];
  for (const node of nodes) {
    if (node.type !== "dir") continue;
    dirs.push(node.relPath);
    dirs.push(...collectSkillFileDirs(node.children));
  }
  return dirs;
}

function SkillFileTreeItem({
  node,
  activeFile,
  collapsedDirs,
  depth,
  onOpenFile,
  onToggleDir,
}: {
  node: SkillFileTreeNode;
  activeFile: string | null;
  collapsedDirs: Set<string>;
  depth: number;
  onOpenFile: (relPath: string) => void;
  onToggleDir: (relPath: string) => void;
}) {
  if (node.type === "dir") {
    const collapsed = collapsedDirs.has(node.relPath);
    return (
      <li>
        <button
          type="button"
          className="flex min-w-max items-center gap-1.5 rounded-lg py-1.5 pr-2 text-left text-xs font-medium text-foreground-secondary transition-colors hover:bg-accent hover:text-foreground"
          style={{ paddingLeft: `${8 + depth * 14}px` }}
          onClick={() => onToggleDir(node.relPath)}
          aria-expanded={!collapsed}
        >
          {collapsed ? (
            <ChevronRight className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
          ) : (
            <ChevronDown className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
          )}
          <Folder className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
          <span className="whitespace-nowrap">{node.name}</span>
        </button>
        {!collapsed && node.children.length > 0 && (
          <ul className="space-y-0.5">
            {node.children.map((child) => (
              <SkillFileTreeItem
                key={child.relPath}
                node={child}
                activeFile={activeFile}
                collapsedDirs={collapsedDirs}
                depth={depth + 1}
                onOpenFile={onOpenFile}
                onToggleDir={onToggleDir}
              />
            ))}
          </ul>
        )}
      </li>
    );
  }

  const active = activeFile === node.relPath;
  return (
    <li>
      <button
        type="button"
        onClick={() => onOpenFile(node.relPath)}
        className={`flex min-w-max items-center gap-2 rounded-lg py-2 pr-2 text-left text-xs transition-colors ${
          active
            ? "bg-accent text-foreground"
            : "text-foreground-secondary hover:bg-accent hover:text-foreground"
        }`}
        style={{ paddingLeft: `${8 + depth * 14}px` }}
      >
        <FilePreviewIcon fileName={node.relPath} />
        <span className="whitespace-nowrap">{node.name}</span>
      </button>
    </li>
  );
}

function FilePreviewIcon({ fileName }: { fileName: string }) {
  const ext = fileName.split(".").pop()?.toLowerCase();
  if (["png", "jpg", "jpeg", "gif", "webp", "svg"].includes(ext ?? "")) {
    return <FileImage className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />;
  }
  if (["json", "toml", "yaml", "yml", "ts", "tsx", "js", "jsx", "rs", "sh"].includes(ext ?? "")) {
    return <FileCode className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />;
  }
  return <FileText className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />;
}

function EmptyPreview() {
  return (
    <div className="grid min-h-[260px] place-items-center px-6 py-8 text-center">
      <div className="max-w-xs">
        <FolderOpen
          className="mx-auto mb-3 h-8 w-8 text-foreground-muted"
          aria-hidden="true"
        />
        <div className="text-sm font-semibold text-foreground">选择文件预览</div>
        <div className="mt-2 text-[13px] leading-6 text-foreground-muted">
          左侧列出该技能目录中的文件，选择后可在这里查看内容。
        </div>
      </div>
    </div>
  );
}

function FilePreviewBody({ preview }: { preview: SkillFilePreview }) {
  if (preview.kind === "markdown") {
    return (
      <div className="px-5 py-4">
        <MarkdownText
          value={preview.text ?? ""}
          className="max-w-full [overflow-wrap:anywhere]"
        />
      </div>
    );
  }
  if (preview.kind === "text") {
    return (
      <pre className="min-h-full overflow-visible p-4 font-mono text-[12px] leading-6 text-foreground-secondary [overflow-wrap:anywhere]">
        <code>{preview.text}</code>
      </pre>
    );
  }
  if (preview.kind === "image" && preview.dataUrl) {
    return (
      <div className="grid min-h-[260px] place-items-center p-4">
        <img src={preview.dataUrl} alt={preview.name} className="max-h-full max-w-full rounded-lg" />
      </div>
    );
  }
  return (
    <div className="grid min-h-[260px] place-items-center px-6 py-8 text-center">
      <div>
        <FileText
          className="mx-auto mb-3 h-8 w-8 text-foreground-muted"
          aria-hidden="true"
        />
        <div className="text-sm font-semibold text-foreground">无法预览</div>
        <div className="mt-2 text-[13px] text-foreground-muted">
          该文件类型不支持在应用内预览。
        </div>
      </div>
    </div>
  );
}
