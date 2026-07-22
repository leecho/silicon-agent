import { useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { FileArchive, FolderOpen, UploadCloud } from "lucide-react";
import { importTeamFromPath, pickDirectory, pickTeamZip } from "../../api";
import { Button } from "../../components/ui/Button";
import { Modal, ModalHeader } from "../../components/ui/Modal";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { Team } from "../../types";

/** 导入团队弹窗：拖拽 / 选 zip / 选目录。导入成功回调 onImported 后由上层关闭并刷新。 */
export function TeamImportModal({
  open,
  onClose,
  onImported,
}: {
  open: boolean;
  onClose: () => void;
  onImported: (team: Team) => void;
}) {
  const notifications = useNotifications();
  const [dragOver, setDragOver] = useState(false);
  const [importing, setImporting] = useState(false);

  async function doImport(path: string) {
    setImporting(true);
    try {
      const team = await importTeamFromPath(path);
      notifications.notify({
        tone: "success",
        title: "导入成功",
        message: `已导入团队「${team.displayName}」，共 ${team.memberCount} 名成员`,
      });
      onImported(team);
    } catch (err) {
      notifications.notify({ tone: "error", title: "导入失败", message: String(err) });
    } finally {
      setImporting(false);
    }
  }

  async function handlePickZip() {
    const path = await pickTeamZip();
    if (path) await doImport(path);
  }

  async function handlePickDir() {
    const path = await pickDirectory();
    if (path) await doImport(path);
  }

  // 仅在弹窗打开时监听窗口级拖拽事件：over/enter 高亮、drop 导入首个路径。
  useEffect(() => {
    if (!open) {
      setDragOver(false);
      return;
    }
    let unlisten: (() => void) | undefined;
    let active = true;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const t = event.payload.type;
        if (t === "over" || t === "enter") {
          setDragOver(true);
        } else if (t === "drop") {
          setDragOver(false);
          const paths = (event.payload as { paths?: string[] }).paths ?? [];
          if (paths.length > 0) void doImport(paths[0]);
        } else {
          setDragOver(false);
        }
      })
      .then((fn) => {
        if (active) unlisten = fn;
        else fn();
      });
    return () => {
      active = false;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  return (
    <Modal open={open} onClose={onClose} title="导入团队">
      <ModalHeader onClose={onClose}>
        <h2 className="text-base font-semibold text-foreground">导入团队</h2>
        <p className="mt-0.5 text-xs text-foreground-muted">
          团队包是包含 lead 与成员定义的目录；支持 .zip 压缩包或文件夹。
        </p>
      </ModalHeader>

      <div
        className={`mt-4 flex flex-col items-center justify-center gap-3 rounded-xl border border-dashed px-6 py-10 text-center transition-colors ${
          dragOver ? "border-primary bg-accent" : "border-border bg-card"
        }`}
      >
        <div className="grid h-12 w-12 place-items-center rounded-full bg-muted text-foreground-muted">
          <UploadCloud className="h-6 w-6" aria-hidden="true" />
        </div>
        <p className="text-sm text-foreground-secondary">
          {importing ? "导入中…" : dragOver ? "松开以导入" : "拖拽 .zip 或团队文件夹到此处"}
        </p>
      </div>

      <div className="mt-4 flex items-center justify-center gap-2">
        <Button tone="primary" onClick={handlePickZip} disabled={importing}>
          <FileArchive className="h-4 w-4" aria-hidden="true" />
          选择 zip
        </Button>
        <Button tone="outline" onClick={handlePickDir} disabled={importing}>
          <FolderOpen className="h-4 w-4" aria-hidden="true" />
          选择文件夹
        </Button>
      </div>
    </Modal>
  );
}
