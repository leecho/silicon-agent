import { useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { FileArchive, FolderOpen, UploadCloud } from "lucide-react";
import { installSkillFromPath, pickDirectory, pickSkillZip } from "../../api";
import { Button } from "../../components/ui/Button";
import { Modal, ModalHeader } from "../../components/ui/Modal";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { Skill } from "../../types";

/** 安装技能弹窗：拖拽 / 选 zip / 选目录。安装成功回调 onInstalled 后由上层关闭并刷新。 */
export function SkillInstallModal({
  open,
  onClose,
  onInstalled,
}: {
  open: boolean;
  onClose: () => void;
  onInstalled: (skill: Skill) => void;
}) {
  const notifications = useNotifications();
  const [dragOver, setDragOver] = useState(false);
  const [installing, setInstalling] = useState(false);

  async function doInstall(path: string) {
    setInstalling(true);
    try {
      const skill = await installSkillFromPath(path);
      notifications.notify({
        tone: "success",
        title: "安装成功",
        message: `已安装技能「${skill.name}」`,
      });
      onInstalled(skill);
    } catch (err) {
      notifications.notify({ tone: "error", title: "安装失败", message: String(err) });
    } finally {
      setInstalling(false);
    }
  }

  async function handlePickZip() {
    const path = await pickSkillZip();
    if (path) await doInstall(path);
  }

  async function handlePickDir() {
    const path = await pickDirectory();
    if (path) await doInstall(path);
  }

  // 仅在弹窗打开时监听窗口级拖拽事件：over/enter 高亮、drop 安装首个路径。
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
          if (paths.length > 0) void doInstall(paths[0]);
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
    <Modal open={open} onClose={onClose} title="安装技能">
      <ModalHeader onClose={onClose}>
        <h2 className="text-base font-semibold text-foreground">安装技能</h2>
        <p className="mt-0.5 text-xs text-foreground-muted">
          技能是一个含 SKILL.md 的目录；支持 .zip 压缩包或文件夹。
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
          {installing ? "安装中…" : dragOver ? "松开以安装" : "拖拽 .zip 或技能文件夹到此处"}
        </p>
      </div>

      <div className="mt-4 flex items-center justify-center gap-2">
        <Button tone="primary" onClick={handlePickZip} disabled={installing}>
          <FileArchive className="h-4 w-4" aria-hidden="true" />
          选择 zip
        </Button>
        <Button tone="outline" onClick={handlePickDir} disabled={installing}>
          <FolderOpen className="h-4 w-4" aria-hidden="true" />
          选择文件夹
        </Button>
      </div>
    </Modal>
  );
}
