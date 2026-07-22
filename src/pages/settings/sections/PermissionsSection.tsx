import { useEffect, useState } from "react";
import { ShieldCheck } from "lucide-react";
import { SystemPermissionsSection } from "./SystemPermissionsSection";
import { getGlobalPermissionMode, setGlobalPermissionMode } from "../../../api";
import { Select, Tooltip, useNotifications } from "../../../components/ui";
import { SettingItem } from "../../../components/settings/SettingsControls";
import type { PermissionMode } from "../../../types";

const PERMISSION_OPTIONS: {
  description: string;
  label: string;
  value: PermissionMode;
}[] = [
  {
    description: "每次工具调用都需要手动审批，适合需要精细控制的场景。",
    label: "手动审批",
    value: "manual",
  },
  {
    description: "低风险操作自动放行，高风险操作仍需审批，平衡效率与安全。",
    label: "自动审批",
    value: "auto",
  },
  {
    description: "所有工具调用均自动放行，适合完全信任的场景。",
    label: "完全权限",
    value: "full",
  },
];

/** 权限设置 section：默认权限模式与系统授权。 */
export function PermissionsSection() {
  const notify = useNotifications();
  const [permissionMode, setPermissionMode] = useState<PermissionMode>("manual");

  useEffect(() => {
    getGlobalPermissionMode().then(setPermissionMode).catch(() => {});
  }, []);

  async function selectPermissionMode(value: PermissionMode) {
    setPermissionMode(value);
    try {
      await setGlobalPermissionMode(value);
    } catch (err) {
      notify.error({ title: "权限模式设置失败", message: String(err) });
    }
  }

  return (
    <section className="grid gap-8" aria-label="权限设置">
      <div className="settings-section-surface overflow-hidden rounded-lg border border-border bg-surface">
        <SettingItem
          title="默认权限模式"
          description="新会话默认的权限强度；可在会话内单独覆盖。"
          icon={ShieldCheck}
        >
          <Select
            className="text-sm h-10 w-full rounded-lg border border-border bg-background px-3 text-foreground outline-none transition focus:border-ring"
            value={permissionMode}
            tooltip="默认权限模式"
            options={PERMISSION_OPTIONS}
            onChange={(value) => { void selectPermissionMode(value as PermissionMode); }}
            renderOption={(option) => (
              <Tooltip content={option.description}>
              <span className="min-w-0">
                <span className="block truncate">{option.label}</span>
              </span>
                </Tooltip>
            )}
          />
        </SettingItem>
      </div>
      <SystemPermissionsSection />
    </section>
  );
}
