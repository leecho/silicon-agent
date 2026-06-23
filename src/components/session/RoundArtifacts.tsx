import { useEffect, useState, type MouseEvent } from "react";
import { Clipboard, ExternalLink, Eye, FolderOpen } from "lucide-react";
import { openArtifactFile, revealArtifactFile } from "../../api";
import type { Artifact } from "../../types";
import {
  DropdownMenu,
  type DropdownMenuPosition,
} from "../ui/DropdownMenu";
import { SplitButton } from "../ui/SplitButton";
import { Tooltip } from "../ui/Tooltip";
import { useNotifications } from "../ui/NotificationProvider";
import {
  artifactFileName,
  artifactFullPath,
  artifactIcon,
} from "./artifactFilePresentation";

// 「本轮产物」汇总块：渲染在某一轮（用户轮）末尾，列出本轮登记的全部产物。
export function RoundArtifacts({
  sessionId,
  artifacts,
  onOpen,
  resolvedWorkingDir,
}: {
  sessionId: string;
  artifacts: Artifact[];
  onOpen?: (a: Artifact) => void;
  resolvedWorkingDir?: string;
}) {
  const notifications = useNotifications();
  const [menuArtifact, setMenuArtifact] = useState<Artifact | null>(null);
  const [menuPosition, setMenuPosition] = useState<DropdownMenuPosition>({
    x: 0,
    y: 0,
  });

  useEffect(() => {
    if (!menuArtifact) return;
    function closeMenu() {
      setMenuArtifact(null);
    }
    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") closeMenu();
    }
    document.addEventListener("click", closeMenu);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("click", closeMenu);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [menuArtifact]);

  function artifactPath(a: Artifact): string | undefined {
    return artifactFullPath(resolvedWorkingDir, a.path);
  }

  function notifyError(title: string, err: unknown) {
    notifications.error({
      title,
      message: err instanceof Error ? err.message : String(err),
    });
  }

  async function handleOpenFile(a: Artifact) {
    setMenuArtifact(null);
    const fullPath = artifactPath(a);
    if (!fullPath) return;
    try {
      await openArtifactFile(sessionId, a.path);
    } catch (err) {
      notifyError("打开失败", err);
    }
  }

  async function handleRevealFile(a: Artifact) {
    setMenuArtifact(null);
    const fullPath = artifactPath(a);
    if (!fullPath) return;
    try {
      await revealArtifactFile(sessionId, a.path);
    } catch (err) {
      notifyError("打开所在文件夹失败", err);
    }
  }

  async function handleCopyPath(a: Artifact) {
    setMenuArtifact(null);
    const fullPath = artifactPath(a);
    if (!fullPath) return;
    try {
      await navigator.clipboard.writeText(fullPath);
      notifications.success({
        title: "已复制路径",
        message: artifactFileName(a.path),
      });
    } catch (err) {
      notifyError("复制路径失败", err);
    }
  }

  function openActionsMenu(a: Artifact, event: MouseEvent<HTMLButtonElement>) {
    event.stopPropagation();
    if (!artifactPath(a)) return;
    const rect = event.currentTarget.getBoundingClientRect();
    setMenuPosition({
      x: Math.max(8, Math.min(rect.right - 184, window.innerWidth - 192)),
      y: Math.max(8, Math.min(rect.bottom + 6, window.innerHeight - 132)),
    });
    setMenuArtifact((current) => (current?.path === a.path ? null : a));
  }

  return (
    <div className="min-w-0 max-w-full rounded-lg  px-3 py-2.5">
      <div className="grid min-w-0 gap-2">
        {artifacts.map((a) => {
          const fileName = artifactFileName(a.path);
          const fullPath = artifactPath(a);
          const Icon = artifactIcon(a.path);
          return (
            <div
              key={a.path}
              className="flex min-w-0 items-center gap-3 rounded-lg border border-border-subtle bg-background px-3 py-3"
            >
              <Tooltip content={a.path}>
                <button
                  type="button"
                  className="flex min-w-0 flex-1 items-center gap-3 text-left"
                  onClick={() => onOpen?.(a)}
                >
                  <span className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-surface text-foreground-secondary">
                    <Icon className="h-4 w-4" aria-hidden="true" />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-semibold text-foreground">
                      {fileName}
                    </span>
                    <span className="mt-0.5 block truncate text-xs text-foreground-muted">
                      点击预览
                    </span>
                  </span>
                </button>
              </Tooltip>
              <SplitButton
                icon={Eye}
                label="预览"
                menuAriaLabel={`更多打开选项：${fileName}`}
                menuDisabled={!fullPath}
                menuTooltip={fullPath ? "更多打开选项" : "未解析到本地路径"}
                onClick={() => onOpen?.(a)}
                onMenuClick={(event) => openActionsMenu(a, event)}
                ton="surface"
                tooltip="预览"
              />
            </div>
          );
        })}
      </div>
      {menuArtifact && artifactPath(menuArtifact) && (
        <DropdownMenu
          position={menuPosition}
          items={[
            {
              icon: ExternalLink,
              id: "open",
              label: "打开",
              onSelect: () => void handleOpenFile(menuArtifact),
            },
            {
              icon: FolderOpen,
              id: "reveal",
              label: "打开所在文件夹",
              onSelect: () => void handleRevealFile(menuArtifact),
            },
            {
              icon: Clipboard,
              id: "copy-path",
              label: "复制路径",
              onSelect: () => void handleCopyPath(menuArtifact),
            },
          ]}
        />
      )}
    </div>
  );
}
