import { useEffect, useState } from "react";
import type { LucideIcon } from "lucide-react";
import {
  Eye,
  MonitorCog,
  Moon,
  PanelRightOpen,
  ShieldCheck,
  Sun,
} from "lucide-react";
import {
  getGlobalPermissionMode,
  getSessionTaskPanelDefaultVisible,
  getShowCompletedProcess,
  setGlobalPermissionMode,
  setSessionTaskPanelDefaultVisible,
  setShowCompletedProcess,
} from "../../../api";
import { Select, Switch, Tooltip, useNotifications } from "../../../components/ui";
import { applyTheme, type ThemePreference } from "../../../lib/theme";
import { ChoiceCard, SettingItem } from "../../../components/settings/SettingsControls";
import type { PermissionMode } from "../../../types";

type ThemeValue = "system" | "light" | "dark";

const THEME_OPTIONS: {
  description: string;
  icon: LucideIcon;
  label: string;
  value: ThemeValue;
}[] = [
  {
    description: "根据操作系统设置自动切换浅色或深色界面。",
    icon: MonitorCog,
    label: "跟随系统",
    value: "system",
  },
  {
    description: "使用明亮背景，适合白天和高亮环境。",
    icon: Sun,
    label: "浅色",
    value: "light",
  },
  {
    description: "使用低亮度界面，适合长时间工作。",
    icon: Moon,
    label: "深色",
    value: "dark",
  },
];

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

function readStoredTheme(): ThemeValue {
  const stored = localStorage.getItem("theme");
  if (stored === "light" || stored === "dark" || stored === "system") return stored;
  return "system";
}

/** 基础偏好 section：主题选择 + 默认权限模式。 */
export function PreferencesSection() {
  const notify = useNotifications();
  const [theme, setTheme] = useState<ThemeValue>(readStoredTheme);
  const [permissionMode, setPermissionMode] = useState<PermissionMode>("manual");
  const [showCompletedProcess, setShowCompletedProcessState] = useState(true);
  const [sessionTaskPanelDefaultVisible, setSessionTaskPanelDefaultVisibleState] =
    useState(true);

  useEffect(() => {
    getGlobalPermissionMode().then(setPermissionMode).catch(() => {});
    getShowCompletedProcess().then(setShowCompletedProcessState).catch(() => {});
    getSessionTaskPanelDefaultVisible()
      .then(setSessionTaskPanelDefaultVisibleState)
      .catch(() => {});
  }, []);

  function selectTheme(value: ThemeValue) {
    setTheme(value);
    applyTheme(value as ThemePreference);
    localStorage.setItem("theme", value);
  }

  async function selectPermissionMode(value: PermissionMode) {
    setPermissionMode(value);
    await setGlobalPermissionMode(value);
  }

  async function toggleShowCompletedProcess(value: boolean) {
    setShowCompletedProcessState(value);
    try {
      await setShowCompletedProcess(value);
    } catch (err) {
      notify.error({ title: "过程展示设置失败", message: String(err) });
      setShowCompletedProcessState(!value);
    }
  }

  async function toggleSessionTaskPanelDefaultVisible(value: boolean) {
    setSessionTaskPanelDefaultVisibleState(value);
    try {
      await setSessionTaskPanelDefaultVisible(value);
    } catch (err) {
      notify.error({ title: "任务面板设置失败", message: String(err) });
      setSessionTaskPanelDefaultVisibleState(!value);
    }
  }

  return (
    <section className="grid gap-8" aria-label="基础偏好">
      <div>
        <h3 className="mb-4 text-base font-semibold text-foreground">主题</h3>
        <div className="grid gap-4 md:grid-cols-3">
          {THEME_OPTIONS.map((option) => (
            <ChoiceCard
              key={option.value}
              description={option.description}
              icon={option.icon}
              selected={option.value === theme}
              title={option.label}
              onClick={() => selectTheme(option.value)}
            />
          ))}
        </div>
      </div>
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
        <SettingItem
          title="显示已完成轮次的思考与执行过程"
          description="关闭后，已完成的历史轮次只显示用户消息和最终回复；当前正在运行的轮次仍显示实时思考与工具执行。"
          icon={Eye}
        >
          <Switch
            checked={showCompletedProcess}
            onChange={(value) => void toggleShowCompletedProcess(value)}
          />
        </SettingItem>
        <SettingItem
          title="默认显示任务面板"
          description="进入会话时默认展开右侧任务面板；关闭后仍可在会话页手动展开。"
          icon={PanelRightOpen}
        >
          <Switch
            checked={sessionTaskPanelDefaultVisible}
            onChange={(value) => void toggleSessionTaskPanelDefaultVisible(value)}
          />
        </SettingItem>
      </div>
    </section>
  );
}
