import { type MouseEvent } from "react";
import { createPortal } from "react-dom";
import { Copy, MessageSquarePlus } from "lucide-react";

const MENU_GAP = 8;
const MENU_ESTIMATED_WIDTH = 168;
const MENU_ESTIMATED_HEIGHT = 36;
const VIEWPORT_GAP = 12;

// 选中文字后浮现的横向菜单：复制 / 添加到对话。按选区矩形定位在其上方居中，越界收敛进视口。
export function SelectionMenu({
  rect,
  onCopy,
  onAdd,
}: {
  rect: DOMRect;
  onCopy: () => void;
  onAdd: () => void;
}) {
  const preferredLeft = rect.left + rect.width / 2 - MENU_ESTIMATED_WIDTH / 2;
  const left = Math.min(
    Math.max(preferredLeft, VIEWPORT_GAP),
    Math.max(VIEWPORT_GAP, window.innerWidth - VIEWPORT_GAP - MENU_ESTIMATED_WIDTH),
  );
  const top = Math.max(VIEWPORT_GAP, rect.top - MENU_GAP - MENU_ESTIMATED_HEIGHT);

  // 按下时阻止默认：避免点击按钮把浏览器选区清掉（mousedown 会先于 click 清选区）。
  const keepSelection = (e: MouseEvent) => e.preventDefault();

  return createPortal(
    <div
      onMouseDown={keepSelection}
      className="fixed z-[130] flex items-center gap-0.5 rounded-lg border border-border bg-popover p-1 text-popover-foreground shadow-xl"
      style={{ left, top }}
    >
      <button
        type="button"
        onClick={onCopy}
        className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-foreground-secondary transition hover:bg-accent hover:text-foreground"
      >
        <Copy className="h-3.5 w-3.5" aria-hidden="true" />
        复制
      </button>
      <button
        type="button"
        onClick={onAdd}
        className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-foreground-secondary transition hover:bg-accent hover:text-foreground"
      >
        <MessageSquarePlus className="h-3.5 w-3.5" aria-hidden="true" />
        添加到对话
      </button>
    </div>,
    document.body,
  );
}
