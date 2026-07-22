import { useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { FileArchive, FolderOpen, UploadCloud } from "lucide-react";
import { installPluginFromPath, pickDirectory, pickPluginZip } from "../../api";
import { Button } from "../../components/ui/Button";
import { Modal, ModalHeader } from "../../components/ui/Modal";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { Plugin } from "../../types";

/** 安装套件弹窗：拖拽 / 选目录。套件须为含 plugin.json 的文件夹（不支持 zip）。 */
export function PluginInstallModal({
  open,
  onClose,
  onInstalled,
}: {
  open: boolean;
  onClose: () => void;
  onInstalled: (plugin: Plugin) => void;
}) {
  const notifications = useNotifications();
  const [dragOver, setDragOver] = useState(false);
  const [installing, setInstalling] = useState(false);

  async function doInstall(path: string) {
    setInstalling(true);
    try {
      const installed = await installPluginFromPath(path);
      // 三体系分立（T108）：按清单文件名分发。团队包 / 专家包都不会出现在插件列表里，
      // 必须明确指引去哪找，否则用户以为没装上。
      if (installed.kind === "team" || installed.kind === "expert") {
        notifications.notify({
          tone: "success",
          title: "安装成功",
          message: `「${installed.displayName}」已装入${
            installed.kind === "team" ? "「团队」页" : "「专家」页"
          }`,
        });
        onClose();
        return;
      }
      notifications.notify({
        tone: "success",
        title: "安装成功",
        message: `套件「${installed.displayName}」装好了，带来 ${installed.skillCount} 个技能`,
      });
      onInstalled(installed);
    } catch (err) {
      notifications.notify({ tone: "error", title: "安装失败", message: String(err) });
    } finally {
      setInstalling(false);
    }
  }

  async function handlePickZip() {
    const path = await pickPluginZip();
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
    <Modal open={open} onClose={onClose} title="安装套件">
      <ModalHeader onClose={onClose}>
        <h2 className="text-base font-semibold text-foreground">安装套件</h2>
        <p className="mt-0.5 text-xs text-foreground-muted">
          把套件文件拖进来，或选择文件；支持 .zip 压缩包或文件夹。
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
          {installing ? "安装中…" : dragOver ? "松手就开始安装" : "把 .zip 或套件文件夹拖到这里"}
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
