import { useEffect, useState } from "react";
import { FileText, Image as ImageIcon, X } from "lucide-react";
import { Modal, Tooltip } from "../ui";
import { loadAttachmentObjectUrl } from "../../lib/attachments";

// 附件卡片：文件图标 + 文件名（两行截断）+ 类型角标。
// 图片卡片可点击 → 弹框预览（由父组件通过 onOpenImage 触发）。
export function AttachmentCard({
  name,
  kind,
  onRemove,
  onOpenImage,
}: {
  name: string;
  kind: "image" | "file";
  onRemove?: () => void;
  onOpenImage?: () => void;
}) {
  const Icon = kind === "image" ? ImageIcon : FileText;
  const clickable = kind === "image" && !!onOpenImage;
  return (
    <div className="group relative flex shrink-0 items-start gap-2 rounded-lg border border-border-subtle bg-card px-3 py-1 pr-5">
      <Tooltip content={clickable ? "点击预览图片" : name}>
        <button
          type="button"
          disabled={!clickable}
          onClick={clickable ? onOpenImage : undefined}
          className={`flex min-w-0 flex-1 items-start gap-2 text-left ${
            clickable ? "cursor-pointer" : "cursor-default"
          }`}
        >
          <Icon className="mt-0.5 h-4 w-4 shrink-0 text-foreground-secondary" aria-hidden="true" />
          <span className="min-w-0 flex-1">
            <span className="break-words text-xs leading-5 text-foreground [display:-webkit-box] [-webkit-box-orient:vertical] [-webkit-line-clamp:2] overflow-hidden">
              {name}
            </span>
            {/* <span className="mt-1 inline-block rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-foreground-muted">
              {badge}
            </span> */}
          </span>
        </button>
      </Tooltip>
      {onRemove && (
        <Tooltip content="移除">
          <button
            type="button"
            aria-label="移除"
            onClick={onRemove}
            className="absolute right-1 top-1 grid h-5 w-5 place-items-center rounded-full bg-muted text-foreground-muted opacity-0 transition hover:text-foreground group-hover:opacity-100"
          >
            <X className="h-3 w-3" aria-hidden="true" />
          </button>
        </Tooltip>
      )}
    </div>
  );
}

// 图片预览弹框：打开时按 (sessionId, relPath) 读取字节生成 object URL。
export function AttachmentImageModal({
  open,
  sessionId,
  relPath,
  name,
  onClose,
}: {
  open: boolean;
  sessionId: string;
  relPath: string | null;
  name?: string;
  onClose: () => void;
}) {
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    if (!open || !relPath) return;
    let revoked = false;
    let objUrl: string | null = null;
    setUrl(null);
    setError(false);
    loadAttachmentObjectUrl(sessionId, relPath)
      .then((u) => {
        objUrl = u;
        if (revoked) URL.revokeObjectURL(u);
        else setUrl(u);
      })
      .catch((e) => {
        console.error(e);
        if (!revoked) setError(true);
      });
    return () => {
      revoked = true;
      if (objUrl) URL.revokeObjectURL(objUrl);
    };
  }, [open, sessionId, relPath]);

  return (
    <Modal open={open} onClose={onClose} title={name}>
      <div className="grid max-h-[80vh] max-w-[80vw] place-items-center p-2">
        {error ? (
          <div className="p-8 text-sm text-foreground-muted">无法加载图片</div>
        ) : url ? (
          <img src={url} alt={name} className="max-h-[78vh] max-w-full object-contain" />
        ) : (
          <div className="p-8 text-sm text-foreground-muted">加载中…</div>
        )}
      </div>
    </Modal>
  );
}
