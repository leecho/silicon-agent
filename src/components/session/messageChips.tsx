import type { ReactNode } from "react";
import { Tooltip } from "../ui/Tooltip";
import { Wrench } from "lucide-react";

// Composer 发出的 chip 在消息文本里以 ⟦…⟧（数学括号）标记：
//   技能 → ⟦技能：名⟧    文件 → ⟦@相对路径⟧
// 这里把这些标记还原成 chip 样式；其余文本仍作为纯文本节点（React 自动转义，无注入）。
const CHIP_MARKER = /⟦([^⟧]+)⟧/g;
const SKILL_PREFIX = "技能：";

function basename(p: string): string {
  const t = p.replace(/[/\\]+$/, "");
  const parts = t.split(/[/\\]/);
  return parts[parts.length - 1] || p;
}

function Chip({ label, title }: { label: string; title?: string }) {
  return (
    <Tooltip content={title} disabled={!title}>
      <span className="mx-0.5 inline-flex items-center rounded-md bg-surface px-1.5 py-1 align-middle text-xs text-foreground-secondary">
        <Wrench className="h-3 w-3 mr-1" />
        {label}
      </span>
    </Tooltip>
  );
}

// 把含 chip 标记的文本渲染为「纯文本段 + chip」混合节点。
export function renderMessageWithChips(content: string): ReactNode {
  if (!content.includes("⟦")) return content;
  const nodes: ReactNode[] = [];
  const re = new RegExp(CHIP_MARKER);
  let last = 0;
  let key = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(content)) !== null) {
    if (m.index > last) nodes.push(content.slice(last, m.index));
    const inner = m[1];
    if (inner.startsWith(SKILL_PREFIX)) {
      const name = inner.slice(SKILL_PREFIX.length);
      nodes.push(<Chip key={key++} label={name} title={`技能：${name}`} />);
    } else if (inner.startsWith("@")) {
      const path = inner.slice(1);
      nodes.push(<Chip key={key++} label={`@${basename(path)}`} title={path} />);
    } else {
      // 未知标记：原样保留（含括号），避免误吞内容。
      nodes.push(m[0]);
    }
    last = m.index + m[0].length;
  }
  if (last < content.length) nodes.push(content.slice(last));
  return nodes;
}
