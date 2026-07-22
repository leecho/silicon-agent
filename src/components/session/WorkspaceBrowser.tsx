import { ExternalLink, FileQuestion, Maximize2, Minimize2 } from "lucide-react";
import { useState } from "react";
import type { ArtifactContent } from "../../api";
import { Tooltip } from "../ui/Tooltip";
import { artifactFileName, artifactIcon } from "./artifactFilePresentation";
import { ArtifactContentView, PreviewErrorCard, PreviewLoading, PreviewStateCard } from "./ArtifactContentView";
import { WorkspaceTab } from "./WorkspaceTab";

// 工作目录浏览器：左侧文件目录树 + 右侧文件预览（带标题栏 + 全屏）。作用域无关（project / agent / session 均可复用），
// 文件列表与读取/打开由父组件按作用域注入。
export function WorkspaceBrowser({
  workspaceLabel,
  workspacePath,
  files,
  loading,
  error,
  truncated,
  onOpenDir,
  onRefresh,
  readFile,
  onOpenFile,
}: {
  workspaceLabel: string;
  workspacePath?: string;
  files: string[];
  loading: boolean;
  error?: string | null;
  truncated: boolean;
  onOpenDir: () => void;
  onRefresh: () => void;
  readFile: (relPath: string) => Promise<ArtifactContent>;
  onOpenFile: (relPath: string) => void;
}) {
  const [selected, setSelected] = useState<string | null>(null);
  const [content, setContent] = useState<ArtifactContent | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [fullscreen, setFullscreen] = useState(false);

  function openFile(relPath: string) {
    setSelected(relPath);
    setContent(null);
    setPreviewError(null);
    setPreviewLoading(true);
    readFile(relPath)
      .then((c) => setContent(c))
      .catch((e) => setPreviewError(String(e)))
      .finally(() => setPreviewLoading(false));
  }

  const fileName = selected ? artifactFileName(selected) : "";
  const FileIcon = selected ? artifactIcon(selected) : FileQuestion;

  const body =
    selected == null ? (
      <PreviewStateCard
        icon={FileQuestion}
        title="选择文件预览"
        message="点击左侧文件目录树中的文件，在此查看内容。"
      />
    ) : previewLoading ? (
      <PreviewLoading label="加载中…" />
    ) : previewError ? (
      <PreviewErrorCard message={previewError} />
    ) : content ? (
      <ArtifactContentView content={content} fileName={fileName} />
    ) : null;

  return (
    <div className="flex h-full min-h-0">
      {/* 左：文件目录树 */}
      <div className="w-[280px] shrink-0 overflow-hidden border-r border-border-subtle">
        <WorkspaceTab
          workspaceLabel={workspaceLabel}
          workspacePath={workspacePath}
          files={files}
          truncated={truncated}
          loading={loading}
          error={error}
          artifacts={[]}
          onOpenDir={onOpenDir}
          onPreviewFile={openFile}
          onRefresh={onRefresh}
        />
      </div>
      {/* 右：预览区域（标题栏 + 内容） */}
      <div className="flex min-w-0 flex-1 flex-col">
        {selected != null && (
          <PreviewHeader
            fileName={fileName}
            icon={FileIcon}
            onOpen={() => onOpenFile(selected)}
            onFullscreen={() => setFullscreen(true)}
          />
        )}
        <div className="min-h-0 flex-1 overflow-auto p-4">{body}</div>
      </div>

      {/* 全屏预览覆盖层 */}
      {fullscreen && selected != null && (
        <div className="fixed inset-0 z-50 flex flex-col bg-background">
          <PreviewHeader
            fileName={fileName}
            icon={FileIcon}
            onOpen={() => onOpenFile(selected)}
            onFullscreen={() => setFullscreen(false)}
            fullscreen
          />
          <div className="min-h-0 flex-1 overflow-auto p-6">{body}</div>
        </div>
      )}
    </div>
  );
}

// 预览标题栏：左=文件图标+名称；右=打开(系统) / 全屏(或退出全屏)。
function PreviewHeader({
  fileName,
  icon: Icon,
  onOpen,
  onFullscreen,
  fullscreen = false,
}: {
  fileName: string;
  icon: typeof FileQuestion;
  onOpen: () => void;
  onFullscreen: () => void;
  fullscreen?: boolean;
}) {
  return (
    <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border-subtle px-3 py-2">
      <div className="session-header flex min-w-0 items-center gap-1.5">
        <Icon className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />
        <span className="min-w-0 truncate text-[13px] font-medium text-foreground" title={fileName}>
          {fileName}
        </span>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <Tooltip content="用系统默认应用打开">
          <button
            type="button"
            onClick={onOpen}
            className="flex items-center gap-1 rounded px-2 py-1 text-[12px] text-foreground-secondary transition hover:bg-accent hover:text-foreground"
          >
            <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" /> 打开
          </button>
        </Tooltip>
        <Tooltip content={fullscreen ? "退出全屏" : "全屏"}>
          <button
            type="button"
            aria-label={fullscreen ? "退出全屏" : "全屏"}
            onClick={onFullscreen}
            className="grid h-7 w-7 place-items-center rounded text-foreground-secondary transition hover:bg-accent hover:text-foreground"
          >
            {fullscreen ? (
              <Minimize2 className="h-3.5 w-3.5" aria-hidden="true" />
            ) : (
              <Maximize2 className="h-3.5 w-3.5" aria-hidden="true" />
            )}
          </button>
        </Tooltip>
      </div>
    </div>
  );
}
