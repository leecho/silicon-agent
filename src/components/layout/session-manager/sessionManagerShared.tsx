import { Bot, MessageSquare, Send, Smartphone } from "lucide-react";
import type { RemoteBinding } from "../../../api/remote";
import type { SessionGroup, SessionInfo } from "../../../types";

/** colorKey -> Tailwind class（字面量 class 不被 purge）。 */
const groupColorClassNames: Record<string, string> = {
  gray: "bg-zinc-400",
  red: "bg-red-500",
  orange: "bg-orange-500",
  yellow: "bg-yellow-400",
  green: "bg-emerald-400",
  blue: "bg-sky-500",
  purple: "bg-indigo-500",
};

const dotClass = (key: string) => groupColorClassNames[key] ?? "bg-zinc-400";

// 分组色点：colorKey 为十六进制（#RRGGBB）时用内联色；否则用旧的字面量 class 映射。
export function GroupDot({ colorKey }: { colorKey: string }) {
  if (colorKey.startsWith("#")) {
    return (
      <span
        className="h-2.5 w-2.5 shrink-0 rounded-full"
        style={{ backgroundColor: colorKey }}
      />
    );
  }
  return <span className={`h-2.5 w-2.5 shrink-0 rounded-full ${dotClass(colorKey)}`} />;
}

export function byUpdatedDesc(a: SessionInfo, b: SessionInfo): number {
  return b.updatedAt.localeCompare(a.updatedAt);
}

// 草稿标题：取 draft_content 去掉附件标记、技能标记还原后的首行；空则「未命名草稿」。
export function draftTitle(s: SessionInfo): string {
  const firstLine = (s.draftContent ?? "")
    .replace(/⟦@[^⟧]+⟧/g, "")
    .replace(/⟦技能：([^⟧]+)⟧/g, "$1")
    .split("\n")
    .map((l) => l.trim())
    .find((l) => l.length > 0);
  return firstLine || "未命名草稿";
}

export function shortPeerId(peerId: string): string {
  if (peerId.length <= 8) return peerId;
  return `${peerId.slice(0, 4)}…${peerId.slice(-2)}`;
}

export type RemoteChannelId = "wechat" | "telegram" | "dingtalk" | "feishu";

export const remoteChannels: Array<{ id: RemoteChannelId; label: string }> = [
  { id: "wechat", label: "微信" },
  { id: "telegram", label: "Telegram" },
  { id: "dingtalk", label: "钉钉" },
  { id: "feishu", label: "飞书" },
];

export function remoteChannelIcon(channel: string) {
  if (channel === "wechat") return <Smartphone className="h-3.5 w-3.5" aria-hidden="true" />;
  if (channel === "telegram") return <Send className="h-3.5 w-3.5" aria-hidden="true" />;
  if (channel === "dingtalk") return <MessageSquare className="h-3.5 w-3.5" aria-hidden="true" />;
  return <Bot className="h-3.5 w-3.5" aria-hidden="true" />;
}

export type RemoteSessionItem = {
  binding: RemoteBinding;
  session: SessionInfo;
};

// 分组表单模式：create=新建分组（创建后把会话移入）；edit=编辑现有分组。
export type GroupForm =
  | { mode: "create"; session: SessionInfo }
  | { mode: "edit"; group: SessionGroup };
