import { useEffect, useState } from "react";

export interface QuoteSelection {
  rect: DOMRect;
  text: string;
}

// 选区两端是否都落在同一个「AI 回复」可划词容器内。
function selectionWithinAssistant(sel: Selection): boolean {
  const anchor = sel.anchorNode;
  const focus = sel.focusNode;
  if (!anchor || !focus) return false;
  const toEl = (node: Node): Element | null =>
    node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement;
  const a = toEl(anchor)?.closest('[data-quote-source="assistant"]');
  const f = toEl(focus)?.closest('[data-quote-source="assistant"]');
  return !!a && a === f;
}

// 监听全局选区，产出可用于「划词菜单」的当前选区（仅限 AI 回复区域）。
export function useSelectionQuote(): {
  selection: QuoteSelection | null;
  clear: () => void;
} {
  const [selection, setSelection] = useState<QuoteSelection | null>(null);

  const clear = () => setSelection(null);

  useEffect(() => {
    const evaluate = () => {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed || sel.rangeCount === 0) {
        setSelection(null);
        return;
      }
      const text = sel.toString().trim();
      if (!text || !selectionWithinAssistant(sel)) {
        setSelection(null);
        return;
      }
      setSelection({ rect: sel.getRangeAt(0).getBoundingClientRect(), text });
    };

    // mouseup 后选区已稳定；延后一拍读取，避开个别浏览器的时序问题。
    const onMouseUp = () => window.setTimeout(evaluate, 0);
    // 选区被折叠（单击/重新选择）即关闭菜单。
    const onSelectionChange = () => {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed) setSelection(null);
    };
    const onScroll = () => setSelection(null);
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setSelection(null);
    };

    document.addEventListener("mouseup", onMouseUp);
    document.addEventListener("selectionchange", onSelectionChange);
    window.addEventListener("scroll", onScroll, true);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mouseup", onMouseUp);
      document.removeEventListener("selectionchange", onSelectionChange);
      window.removeEventListener("scroll", onScroll, true);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  return { selection, clear };
}
