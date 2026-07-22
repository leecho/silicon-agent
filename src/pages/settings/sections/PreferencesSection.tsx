import { useEffect, useState } from "react";
import type { LucideIcon } from "lucide-react";
import {
  Eye,
  Lightbulb,
  MonitorCog,
  Moon,
  PanelRightOpen,
  Sun,
} from "lucide-react";
import {
  getSessionTaskPanelDefaultVisible,
  getShowCompletedProcess,
  getSuggestionsEnabled,
  setSessionTaskPanelDefaultVisible,
  setShowCompletedProcess,
  setSuggestionsEnabled,
} from "../../../api";
import { Switch, useNotifications } from "../../../components/ui";
import { applyTheme, type ThemePreference } from "../../../lib/theme";
import { ChoiceCard, SettingItem } from "../../../components/settings/SettingsControls";

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

function readStoredTheme(): ThemeValue {
  const stored = localStorage.getItem("theme");
  if (stored === "light" || stored === "dark" || stored === "system") return stored;
  return "system";
}

/** 常规 section：主题、界面显示偏好与快捷建议。 */
export function PreferencesSection() {
  const notify = useNotifications();
  const [theme, setTheme] = useState<ThemeValue>(readStoredTheme);
  const [showCompletedProcess, setShowCompletedProcessState] = useState(true);
  const [sessionTaskPanelDefaultVisible, setSessionTaskPanelDefaultVisibleState] =
    useState(true);
  const [suggestionsOn, setSuggestionsOn] = useState(true);

  useEffect(() => {
    getShowCompletedProcess().then(setShowCompletedProcessState).catch(() => {});
    getSessionTaskPanelDefaultVisible()
      .then(setSessionTaskPanelDefaultVisibleState)
      .catch(() => {});
    getSuggestionsEnabled().then(setSuggestionsOn).catch(() => {});
  }, []);

  function selectTheme(value: ThemeValue) {
    setTheme(value);
    applyTheme(value as ThemePreference);
    localStorage.setItem("theme", value);
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

  async function toggleSuggestions(value: boolean) {
    setSuggestionsOn(value);
    try {
      await setSuggestionsEnabled(value);
    } catch (err) {
      notify.error({ title: "快捷建议设置失败", message: String(err) });
      setSuggestionsOn(!value);
    }
  }

  return (
    <section className="grid gap-8" aria-label="常规">
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
        <SettingItem
          title="快捷建议"
          description="每轮结束后用大模型生成「下一步」建议，点击可填入输入框。关闭可省一次模型调用。"
          icon={Lightbulb}
        >
          <Switch checked={suggestionsOn} onChange={(value) => void toggleSuggestions(value)} />
        </SettingItem>
      </div>
    </section>
  );
}
