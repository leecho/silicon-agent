import {
  FolderInput,
  FolderMinus,
  Pencil,
  Pin,
  PinOff,
  Plus,
  Trash2,
} from "lucide-react";
import {
  DropdownMenu,
  type DropdownMenuEntry,
} from "../../../components/ui";
import type { SessionGroup, SessionInfo } from "../../../types";
import { GroupDot } from "./sessionManagerShared";

export function SessionActionMenu({
  groups,
  menuPosition,
  menuSession,
  onDelete,
  onMoveToGroup,
  onNewGroup,
  onRename,
  onTogglePinned,
}: {
  groups: SessionGroup[];
  menuPosition: { x: number; y: number };
  menuSession: SessionInfo;
  onDelete: (session: SessionInfo) => void;
  onMoveToGroup: (session: SessionInfo, groupId: string | null) => void;
  onNewGroup: (session: SessionInfo) => void;
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
    {
      children: [
        ...(groups.length === 0
          ? [{
              id: "empty-groups",
              render: (
                <div className="px-2.5 py-1.5 text-[12px] text-foreground-muted">
                  暂无分组
                </div>
              ),
              type: "custom" as const,
            }]
          : groups.map((g): DropdownMenuEntry => ({
              id: `group-${g.id}`,
              render: (_entry, state) => {
                const selected = menuSession.groupId === g.id;
                return (
                  <button
                    type="button"
                    onClick={() => {
                      state.close();
                      onMoveToGroup(menuSession, g.id);
                    }}
                    className={`flex h-8 w-full items-center gap-2.5 rounded-[8px] px-2.5 text-left text-[13px] transition ${
                      selected
                        ? "bg-primary text-white"
                        : "text-foreground-secondary hover:bg-muted"
                    }`}
                  >
                    <span className="min-w-0 flex-1 truncate">{g.label}</span>
                    <GroupDot colorKey={g.colorKey} />
                  </button>
                );
              },
              type: "custom",
            }))),
        { id: "new-group-separator", type: "separator" },
        {
          icon: Plus,
          id: "new-group",
          label: "新建分组…",
          onSelect: () => onNewGroup(menuSession),
        },
      ],
      emptyLabel: "暂无分组",
      id: "move-to-group",
      icon: FolderInput,
      label: "移入分组",
    },
  ];
  if (menuSession.groupId) {
    menuItems.push({
      icon: FolderMinus,
      id: "remove-from-group",
      label: "移出分组",
      onSelect: () => onMoveToGroup(menuSession, null),
    });
  }
  menuItems.push(
    { id: "delete-separator", type: "separator" },
    {
      danger: true,
      icon: Trash2,
      id: "delete",
      label: "删除",
      onSelect: () => onDelete(menuSession),
    },
  );

  return (
    <DropdownMenu position={menuPosition} items={menuItems} />
  );
}
