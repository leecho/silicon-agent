import { Button, ColorPicker, Modal } from "../../../components/ui";
import type { GroupForm } from "./sessionManagerShared";

export function GroupFormModal({
  groupColor,
  groupForm,
  groupName,
  onClose,
  onConfirm,
  onGroupColorChange,
  onGroupNameChange,
}: {
  groupColor: string;
  groupForm: GroupForm | null;
  groupName: string;
  onClose: () => void;
  onConfirm: () => void;
  onGroupColorChange: (color: string) => void;
  onGroupNameChange: (name: string) => void;
}) {
  return (
    <Modal
      open={groupForm !== null}
      className="w-auto"
      onClose={onClose}
      title={groupForm?.mode === "edit" ? "编辑分组" : "新建分组"}
    >
      <div className="flex w-80 flex-col gap-4 p-4">
        <label className="flex flex-col gap-1.5">
          <span className="text-xs text-foreground-muted">分组名</span>
          <input
            type="text"
            autoFocus
            value={groupName}
            onChange={(e) => onGroupNameChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") onConfirm();
            }}
            className="rounded-md border border-border-subtle bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-border"
          />
        </label>
        <div className="flex flex-col gap-1.5">
          <span className="text-xs text-foreground-muted">颜色</span>
          <ColorPicker value={groupColor} onChange={onGroupColorChange} />
        </div>
        <div className="flex items-center justify-end gap-2">
          <Button tone="outline" onClick={onClose}>
            取消
          </Button>
          <Button
            tone="primary"
            disabled={!groupName.trim()}
            onClick={onConfirm}
          >
            {groupForm?.mode === "edit" ? "保存" : "创建"}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
