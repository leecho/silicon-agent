import { Pencil, Pin, PinOff, Trash2 } from "lucide-react";

import { DropdownMenu, type DropdownMenuEntry } from "../../../components/ui";
import type { SessionInfo } from "../../../types";

export function AgentSessionActionMenu({
  menuPosition,
  menuSession,
  onDelete,
  onRename,
  onTogglePinned,
}: {
  menuPosition: { x: number; y: number };
  menuSession: SessionInfo;
  onDelete: (session: SessionInfo) => void;
  onRename: (session: SessionInfo) => void;
  onTogglePinned: (session: SessionInfo) => void;
}) {
  const menuItems: DropdownMenuEntry[] = [
    {
      icon: Pencil,
      id: "rename",
      label: "重命名",
      onSelect: () => onRename(menuSession),
    },
    {
      icon: menuSession.pinned ? PinOff : Pin,
      id: "toggle-pinned",
      label: menuSession.pinned ? "取消置顶" : "置顶",
      onSelect: () => onTogglePinned(menuSession),
    },
    { id: "delete-separator", type: "separator" },
    {
      danger: true,
      icon: Trash2,
      id: "delete",
      label: "删除",
      onSelect: () => onDelete(menuSession),
    },
  ];

  return <DropdownMenu position={menuPosition} items={menuItems} />;
}
