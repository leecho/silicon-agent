import { useEffect, useState, type ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { AlertTriangle, FileQuestion, Loader2 } from "lucide-react";
import { MarkdownText } from "../ui/MarkdownText";
import type { ArtifactContent } from "../../api";

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

export function readPreviewThemeCss(): string {
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

/** 预览状态卡（错误 / 二进制占位）。 */
export function PreviewStateCard({
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

/** 按 kind 富渲染产物 / 知识库内容。binaryAction 供二进制态提供「系统打开」按钮。 */
export function ArtifactContentView({
  content,
  fileName,
  binaryAction,
}: {
  content: ArtifactContent;
  fileName: string;
  binaryAction?: ReactNode;
}) {
  const [previewThemeCss, setPreviewThemeCss] = useState(readPreviewThemeCss);

  useEffect(() => {
    if (typeof document === "undefined") return;
    const refresh = () => setPreviewThemeCss(readPreviewThemeCss());
    refresh();
    const observer = new MutationObserver(refresh);
    observer.observe(document.documentElement, {
      attributeFilter: ["class"],
      attributes: true,
    });
    return () => observer.disconnect();
  }, []);

  if (content.kind === "markdown") {
    return (
      <div className="mx-auto max-w-[820px] rounded-lg   px-5 py-4">
        <MarkdownText value={content.content} className="max-w-full [overflow-wrap:anywhere]" />
      </div>
    );
  }
  if (content.kind === "text") {
    return (
      <div className="rounded-lg  bg-surface">
        <pre className="max-w-full overflow-auto whitespace-pre-wrap px-4 py-3 font-mono text-[12px] leading-6 text-foreground-secondary [overflow-wrap:anywhere]">
          {content.content}
        </pre>
      </div>
    );
  }
  if (content.kind === "pdf") {
    return (
      <div className="h-full overflow-hidden rounded-lg  bg-surface">
        <iframe
          data-pdf-preview
          title={fileName}
          src={content.content}
          className="h-full w-full bg-background"
        />
      </div>
    );
  }
  if (content.kind === "html") {
    return (
      <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-lg  bg-surface">
        <iframe
          data-html-preview
          sandbox="allow-scripts"
          title={fileName}
          srcDoc={withHtmlNetworkPreviewCsp(content.content)}
          className="min-h-0 flex-1 bg-background"
        />
      </div>
    );
  }
  if (content.kind === "office") {
    return (
      <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-lg  bg-surface">
        <iframe
          data-office-preview
          sandbox=""
          title={fileName}
          srcDoc={withThemedStaticPreviewCsp(content.content, previewThemeCss)}
          className="min-h-0 flex-1 bg-background"
        />
      </div>
    );
  }
  // binary
  return (
    <PreviewStateCard
      action={binaryAction}
      icon={FileQuestion}
      message="该文件可能是二进制格式，可以用系统默认应用打开查看。"
      title="无法在应用内预览"
    />
  );
}

/** 错误态卡片（供各 drawer 直接复用）。 */
export function PreviewErrorCard({ message }: { message: string }) {
  return <PreviewStateCard icon={AlertTriangle} message={message} title="预览错误" tone="error" />;
}

/** 加载态。 */
export function PreviewLoading({ label }: { label: string }) {
  return (
    <div className="flex items-center gap-2 py-8 text-sm text-foreground-muted">
      <Loader2 className="h-4 w-4 animate-spin" /> {label}
    </div>
  );
}
