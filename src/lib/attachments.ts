import { readAttachment } from "../api";

// Composer 顶部附件区的一条附件。
export interface Attachment {
  id: string;
  /** 工作区相对路径（agent 可访问、序列化为 @相对路径）。 */
  relPath: string;
  /** 展示用文件名（basename）。 */
  name: string;
  kind: "image" | "file";
}

const IMAGE_EXT = new Set([
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "bmp",
  "svg",
  "avif",
  "ico",
]);

export function basename(p: string): string {
  const t = p.replace(/[/\\]+$/, "");
  const parts = t.split(/[/\\]/);
  return parts[parts.length - 1] || p;
}

/** 扩展名（小写，无点）；无扩展名返回空串。 */
export function extname(name: string): string {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "";
}

export function attachmentKind(name: string): "image" | "file" {
  return IMAGE_EXT.has(extname(name)) ? "image" : "file";
}

// 消息文本里的附件以 ⟦@相对路径⟧ 标记（与技能 ⟦技能：名⟧ 区分）。
const ATTACHMENT_MARKER = /⟦@([^⟧]+)⟧/g;

/** 把消息文本拆成「附件列表 + 去掉附件标记后的正文」（正文仍含技能标记，交给 messageChips 渲染）。 */
export function extractAttachments(content: string): {
  attachments: { relPath: string; name: string; kind: "image" | "file" }[];
  body: string;
} {
  const attachments: { relPath: string; name: string; kind: "image" | "file" }[] = [];
  const body = content
    .replace(ATTACHMENT_MARKER, (_m, p: string) => {
      const name = basename(p);
      attachments.push({ relPath: p, name, kind: attachmentKind(name) });
      return "";
    })
    // 附件标记是提交时前置的整行，去掉后清理残留的前导空行。
    .replace(/^\s*\n/, "")
    .replace(/^\n+/, "");
  return { attachments, body };
}

/** 读取附件字节并生成可用于 <img src> 的 object URL（调用方负责 revoke）。 */
export async function loadAttachmentObjectUrl(
  sessionId: string,
  relPath: string,
): Promise<string> {
  const bytes = await readAttachment(sessionId, relPath);
  const blob = new Blob([new Uint8Array(bytes)]);
  return URL.createObjectURL(blob);
}
