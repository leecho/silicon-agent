import {
  useEffect,
  useState,
  type MouseEvent,
} from "react";
import {
  Clipboard,
  ExternalLink,
  FileQuestion,
  FolderOpen,
  Loader2,
} from "lucide-react";
import { Drawer, DrawerHeader } from "../ui/Drawer";
import {
  DropdownMenu,
  type DropdownMenuPosition,
} from "../ui/DropdownMenu";
import { useNotifications } from "../ui/NotificationProvider";
import { SplitButton } from "../ui/SplitButton";
import {
  openArtifactFile,
  readArtifact,
  revealArtifactFile,
  type ArtifactContent,
} from "../../api";
import type { Artifact } from "../../types";
import {
  artifactFileName,
  artifactFullPath,
  artifactIcon,
} from "./artifactFilePresentation";
import { ArtifactContentView, PreviewErrorCard } from "./ArtifactContentView";

// 产物预览右侧抽屉：md 渲染 / 纯文本 / 二进制系统打开。artifact 为空时不渲染。
export function ArtifactPreviewDrawer({
  sessionId,
  artifact,
  resolvedWorkingDir,
  onClose,
}: {
  sessionId: string;
  artifact: Artifact | null;
  resolvedWorkingDir?: string;
  onClose: () => void;
}) {
  const notifications = useNotifications();
  const [state, setState] = useState<
    | { phase: "loading" }
    | { phase: "error"; message: string }
    | { phase: "ready"; content: ArtifactContent }
  >({ phase: "loading" });
  const [menuOpen, setMenuOpen] = useState(false);
  const [menuPosition, setMenuPosition] = useState<DropdownMenuPosition>({
    x: 0,
    y: 0,
  });

  useEffect(() => {
    if (!artifact) return;
    setState({ phase: "loading" });
    let cancelled = false;
    readArtifact(sessionId, artifact.path)
      .then((content) => {
        if (!cancelled) setState({ phase: "ready", content });
      })
      .catch((err) => {
        if (!cancelled) setState({ phase: "error", message: String(err) });
      });
    return () => {
      cancelled = true;
    };
  }, [sessionId, artifact]);

  useEffect(() => {
    setMenuOpen(false);
  }, [artifact]);

  useEffect(() => {
    if (!menuOpen) return;
    function closeMenu() {
      setMenuOpen(false);
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
  }, [menuOpen]);

  const fullPath = artifactFullPath(resolvedWorkingDir, artifact?.path);
  const fileName = artifact ? artifactFileName(artifact.path) : "产物";
  const Icon = artifact ? artifactIcon(artifact.path) : FileQuestion;

  function openActionsMenu(event: MouseEvent<HTMLButtonElement>) {
    event.stopPropagation();
    if (!artifact) return;
    const rect = event.currentTarget.getBoundingClientRect();
    setMenuPosition({
      x: Math.max(8, Math.min(rect.right - 184, window.innerWidth - 192)),
      y: Math.max(8, Math.min(rect.bottom + 6, window.innerHeight - 132)),
    });
    setMenuOpen((open) => !open);
  }

  async function handleOpenFile() {
    if (!artifact) return;
    setMenuOpen(false);
    try {
      await openArtifactFile(sessionId, artifact.path);
    } catch (err) {
      notifications.error({
        title: "打开失败",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  async function handleRevealFile() {
    if (!artifact) return;
    setMenuOpen(false);
    try {
      await revealArtifactFile(sessionId, artifact.path);
    } catch (err) {
      notifications.error({
        title: "打开所在文件夹失败",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  async function handleCopyPath() {
    if (!fullPath) return;
    setMenuOpen(false);
    try {
      await navigator.clipboard.writeText(fullPath);
      notifications.success({ title: "已复制路径", message: fileName });
    } catch (err) {
      notifications.error({
        title: "复制路径失败",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  return (
    <Drawer
      className="w-[min(1040px,94vw)] bg-background"
      open={!!artifact}
      onClose={onClose}
      title={fileName}
    >
      <DrawerHeader onClose={onClose}>
        <div className="flex min-w-0 items-center gap-3 justify-between">
          <div className="flex items-center gap-3 min-w-0">
            <div className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-card text-foreground-secondary">
              <Icon className="h-4 w-4" aria-hidden="true" />
            </div>
            <h2 className="min-w-0 flex-1 truncate text-base font-semibold text-foreground">
              {fileName}
            </h2>
          </div>
          <SplitButton
            disabled={!artifact}
            icon={ExternalLink}
            label="打开"
            menuAriaLabel={`更多打开选项：${fileName}`}
            menuDisabled={!artifact}
            menuTooltip="更多打开选项"
            onClick={() => void handleOpenFile()}
            onMenuClick={openActionsMenu}
            ton="card"
            tooltip="打开"
          />
        </div>
      </DrawerHeader>
      <div className="flex h-full min-h-0 flex-col bg-background">
        <div className="min-h-0 flex-1 overflow-auto px-5 py-4">
          {state.phase === "loading" && (
            <div className="grid h-full place-items-center text-sm text-foreground-muted">
              <div className="flex items-center gap-2">
                <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
                加载中...
              </div>
            </div>
          )}
          {state.phase === "error" && <PreviewErrorCard message={state.message} />}
          {state.phase === "ready" && (
            <ArtifactContentView
              content={state.content}
              fileName={fileName}
              binaryAction={
                <button
                  type="button"
                  disabled={!artifact}
                  className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border bg-background px-3 text-sm font-medium text-foreground transition hover:bg-accent disabled:cursor-not-allowed disabled:opacity-45"
                  onClick={() => void handleOpenFile()}
                >
                  <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
                  打开
                </button>
              }
            />
          )}
        </div>
      </div>
      {menuOpen && artifact && (
        <DropdownMenu
          position={menuPosition}
          items={[
            {
              icon: FolderOpen,
              id: "reveal",
              label: "打开所在文件夹",
              onSelect: () => void handleRevealFile(),
            },
            {
              disabled: !fullPath,
              icon: Clipboard,
              id: "copy-path",
              label: "复制路径",
              onSelect: () => void handleCopyPath(),
              tooltip: fullPath ? undefined : "未解析到本地路径",
            },
          ]}
        />
      )}
    </Drawer>
  );
}
