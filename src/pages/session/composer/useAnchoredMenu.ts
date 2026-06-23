import { useEffect, useRef, useState } from "react";
import type { DropdownMenuAnchor, DropdownMenuPosition } from "../../../components/ui";

// Composer 各下拉共用：管理开合 + 记录触发按钮位置。
// DropdownMenu 会基于真实菜单尺寸完成上下翻转和视口内约束。
export function useAnchoredMenu(menuWidth = 184, estHeight = 120) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<DropdownMenuPosition>({ x: 0, y: 0 });
  const [anchorRect, setAnchorRect] = useState<DropdownMenuAnchor | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);

  const updateAnchorRect = () => {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (!rect) return;
    setAnchorRect({
      bottom: rect.bottom,
      left: rect.left,
      right: rect.right,
      top: rect.top,
    });
    setPos({
      x: Math.max(8, Math.min(rect.left, window.innerWidth - menuWidth - 8)),
      y: Math.max(8, rect.top - estHeight),
    });
  };

  const openMenu = () => {
    updateAnchorRect();
    setOpen(true);
  };

  const toggle = () => (open ? setOpen(false) : openMenu());
  const close = () => setOpen(false);

  useEffect(() => {
    if (!open) return;

    const handleViewportChange = () => updateAnchorRect();
    window.addEventListener("resize", handleViewportChange);
    window.addEventListener("scroll", handleViewportChange, true);
    return () => {
      window.removeEventListener("resize", handleViewportChange);
      window.removeEventListener("scroll", handleViewportChange, true);
    };
  }, [open]);

  return { anchorRect, open, pos, triggerRef, openMenu, toggle, close };
}
