import {
  useEffect,
  useState,
  type MouseEvent,
  type ReactNode,
} from "react";
import type { LucideIcon } from "lucide-react";
import {
  AlertTriangle,
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
import { MarkdownText } from "../ui/MarkdownText";
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

function PreviewStateCard({
  action,
  icon: StatusIcon,
  message,
  title,
  tone = "neutral",
}: {
  action?: ReactNode;
  icon: LucideIcon;
  message: ReactNode;
  title: string;
  tone?: "error" | "neutral";
}) {
  return (
    <div className="grid h-full place-items-center rounded-lg  bg-card px-5 py-8 text-center">
      <div className="max-w-sm">
        <StatusIcon
          className={`mx-auto mb-3 h-8 w-8 ${
            tone === "error" ? "text-destructive" : "text-foreground-muted"
          }`}
          aria-hidden="true"
        />
        <div className="text-sm font-semibold text-foreground">{title}</div>
        <div className="mt-2 break-words text-[13px] leading-6 text-foreground-muted">
          {message}
        </div>
        {action && <div className="mt-4 flex justify-center">{action}</div>}
      </div>
    </div>
  );
}

const HTML_PREVIEW_CSP = [
  "default-src 'none'",
  "script-src 'unsafe-inline' https:",
  "style-src 'unsafe-inline' https:",
  "img-src data: blob: https:",
  "font-src data: https:",
  "media-src data: blob:",
  "connect-src https:",
  "frame-src 'none'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
].join("; ");

const STATIC_PREVIEW_CSP = [
  "default-src 'none'",
  "style-src 'unsafe-inline'",
  "img-src data:",
  "font-src data:",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
].join("; ");

function readPreviewThemeCss(): string {
  if (typeof document === "undefined" || typeof window === "undefined") {
    return "";
  }
  const root = document.documentElement;
  const styles = window.getComputedStyle(root);
  const readToken = (name: string, fallback: string) =>
    styles.getPropertyValue(name).trim() || fallback;
  const colorScheme = document.documentElement.classList.contains("theme-dark")
    ? "dark"
    : "light";

  return `
:root {
  color-scheme: ${colorScheme};
  --preview-bg-page: ${readToken("--background", "#ffffff")};
  --preview-bg-panel: ${readToken("--card", "#f7f7f6")};
  --preview-bg-raised: ${readToken("--popover", "#ffffff")};
  --preview-bg-hover: ${readToken("--accent", "rgb(0 0 0 / 5%)")};
  --preview-text-primary: ${readToken("--foreground", "#18181b")};
  --preview-text-secondary: ${readToken("--foreground-secondary", "#5f6368")};
  --preview-text-tertiary: ${readToken("--foreground-muted", "#8a8d91")};
  --preview-border-subtle: ${readToken("--border-subtle", "rgb(0 0 0 / 5%)")};
  --preview-border-default: ${readToken("--border", "rgb(0 0 0 / 8%)")};
}
html,
body {
  background: var(--preview-bg-page) !important;
  color: var(--preview-text-primary) !important;
}
main {
  background: var(--preview-bg-page) !important;
}
header {
  border-bottom-color: var(--preview-border-default) !important;
}
.muted {
  color: var(--preview-text-secondary) !important;
}
.notice,
.slide {
  background: var(--preview-bg-panel) !important;
  border-color: var(--preview-border-default) !important;
}
td,
th {
  border-color: var(--preview-border-default) !important;
}
`;
}

function withHtmlNetworkPreviewCsp(html: string): string {
  const meta = `<meta http-equiv="Content-Security-Policy" content="${HTML_PREVIEW_CSP}">`;
  if (/<head\b[^>]*>/i.test(html)) {
    return html.replace(/<head\b[^>]*>/i, (head) => `${head}${meta}`);
  }
  return `${meta}${html}`;
}

function withThemedStaticPreviewCsp(html: string, previewThemeCss: string): string {
  const meta = `<meta http-equiv="Content-Security-Policy" content="${STATIC_PREVIEW_CSP}">`;
  const themeStyle = `<style data-app-preview-theme>${previewThemeCss}</style>`;
  if (/<head\b[^>]*>/i.test(html)) {
    return html.replace(/<head\b[^>]*>/i, (head) => `${head}${meta}${themeStyle}`);
  }
  return `${meta}${themeStyle}${html}`;
}

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
  const [previewThemeCss, setPreviewThemeCss] = useState(readPreviewThemeCss);

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
    if (typeof document === "undefined") return;
    const refreshPreviewTheme = () => setPreviewThemeCss(readPreviewThemeCss());
    refreshPreviewTheme();
    const observer = new MutationObserver(refreshPreviewTheme);
    observer.observe(document.documentElement, {
      attributeFilter: ["class"],
      attributes: true,
    });
    return () => observer.disconnect();
  }, []);

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
          {state.phase === "error" && (
            <PreviewStateCard
              icon={AlertTriangle}
              message={state.message}
              title="预览错误"
              tone="error"
            />
          )}
          {state.phase === "ready" && state.content.kind === "markdown" && (
            <div className="mx-auto max-w-[820px] rounded-lg   px-5 py-4">
              <MarkdownText
                value={state.content.content}
                className="max-w-full [overflow-wrap:anywhere]"
              />
            </div>
          )}
          {state.phase === "ready" && state.content.kind === "text" && (
            <div className="rounded-lg  bg-surface">
              <pre className="max-w-full overflow-auto whitespace-pre-wrap px-4 py-3 font-mono text-[12px] leading-6 text-foreground-secondary [overflow-wrap:anywhere]">
                {state.content.content}
              </pre>
            </div>
          )}
          {state.phase === "ready" && state.content.kind === "pdf" && (
            <div className="h-full overflow-hidden rounded-lg  bg-surface">
              <iframe
                data-pdf-preview
                title={fileName}
                src={state.content.content}
                className="h-full w-full bg-background"
              />
            </div>
          )}
          {state.phase === "ready" && state.content.kind === "html" && (
            <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-lg  bg-surface">
              <iframe
                data-html-preview
                sandbox="allow-scripts"
                title={fileName}
                srcDoc={withHtmlNetworkPreviewCsp(state.content.content)}
                className="min-h-0 flex-1 bg-background"
              />
            </div>
          )}
          {state.phase === "ready" && state.content.kind === "office" && (
            <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-lg  bg-surface">
              <iframe
                data-office-preview
                sandbox=""
                title={fileName}
                srcDoc={withThemedStaticPreviewCsp(state.content.content, previewThemeCss)}
                className="min-h-0 flex-1 bg-background"
              />
            </div>
          )}
          {state.phase === "ready" && state.content.kind === "binary" && (
            <PreviewStateCard
              action={
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
              icon={FileQuestion}
              message="该文件可能是二进制格式，可以用系统默认应用打开查看。"
              title="无法在应用内预览"
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
