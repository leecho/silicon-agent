import { useEffect, useState } from "react";
import { EyeOff, Globe, MonitorPlay, Timer } from "lucide-react";
import {
  getBrowserHeadless,
  getBrowserIdleCloseMin,
  getBrowserUseEnabled,
  getComputerUseEnabled,
  setBrowserHeadless,
  setBrowserIdleCloseMin,
  setBrowserUseEnabled,
  setComputerUseEnabled,
} from "../../../api";
import { Select, Switch, useNotifications } from "../../../components/ui";
import { SettingItem } from "../../../components/settings/SettingsControls";

const IDLE_CLOSE_OPTIONS = [
  { label: "5 分钟", value: "5" },
  { label: "10 分钟（默认）", value: "10" },
  { label: "30 分钟", value: "30" },
  { label: "不自动关闭", value: "0" },
];

/** 自动化 section：桌面操作、浏览器操作开关与静默模式。 */
export function AutomationSection() {
  const notify = useNotifications();
  const [computerUseEnabled, setComputerUseEnabledState] = useState(false);
  const [browserUseEnabled, setBrowserUseEnabledState] = useState(false);
  const [browserHeadless, setBrowserHeadlessState] = useState(false);
  const [browserIdleCloseMin, setBrowserIdleCloseMinState] = useState(10);

  useEffect(() => {
    getComputerUseEnabled().then(setComputerUseEnabledState).catch(() => {});
    getBrowserUseEnabled().then(setBrowserUseEnabledState).catch(() => {});
    getBrowserHeadless().then(setBrowserHeadlessState).catch(() => {});
    getBrowserIdleCloseMin().then(setBrowserIdleCloseMinState).catch(() => {});
  }, []);

  async function toggleComputerUse(value: boolean) {
    setComputerUseEnabledState(value);
    try {
      await setComputerUseEnabled(value);
    } catch (err) {
      notify.error({ title: "桌面操作设置失败", message: String(err) });
      setComputerUseEnabledState(!value);
    }
  }

  async function toggleBrowserUse(value: boolean) {
    setBrowserUseEnabledState(value);
    try {
      await setBrowserUseEnabled(value);
    } catch (err) {
      notify.error({ title: "浏览器操作设置失败", message: String(err) });
      setBrowserUseEnabledState(!value);
    }
  }

  async function toggleBrowserHeadless(value: boolean) {
    setBrowserHeadlessState(value);
    try {
      await setBrowserHeadless(value);
    } catch (err) {
      notify.error({ title: "静默模式设置失败", message: String(err) });
      setBrowserHeadlessState(!value);
    }
  }

  async function changeBrowserIdleCloseMin(min: number) {
    const prev = browserIdleCloseMin;
    setBrowserIdleCloseMinState(min);
    try {
      await setBrowserIdleCloseMin(min);
    } catch (err) {
      notify.error({ title: "空闲关闭设置失败", message: String(err) });
      setBrowserIdleCloseMinState(prev);
    }
  }

  return (
    <section className="grid gap-8" aria-label="自动化">
      <div className="settings-section-surface overflow-hidden rounded-lg border border-border bg-surface">
        <SettingItem
          title="桌面操作"
          description="允许 AI 读取你的屏幕并自主点击、输入来完成任务（仅 macOS，需在系统设置授予辅助功能权限）。"
          icon={MonitorPlay}
        >
          <Switch
            checked={computerUseEnabled}
            onChange={(value) => void toggleComputerUse(value)}
          />
        </SettingItem>
        <SettingItem
          title="浏览器操作"
          description="允许 AI 打开一个自动化浏览器窗口，帮你在网页上完成任务（需本机安装 Chrome；提交等关键操作会先征求你同意）。"
          icon={Globe}
        >
          <Switch
            checked={browserUseEnabled}
            onChange={(value) => void toggleBrowserUse(value)}
          />
        </SettingItem>
        {browserUseEnabled && (
          <SettingItem
            title="静默模式"
            description="不弹出可见窗口，AI 在后台浏览；建议仅用于抓取/检索，操作类任务建议关闭以便你随时查看。"
            icon={EyeOff}
          >
            <Switch
              checked={browserHeadless}
              onChange={(value) => void toggleBrowserHeadless(value)}
            />
          </SettingItem>
        )}
        {browserUseEnabled && (
          <SettingItem
            title="空闲自动关闭"
            description="浏览器开启后会一直常驻供后续任务复用；超过设定的空闲时间没用就自动关闭。0 = 不自动关。"
            icon={Timer}
          >
            <Select
              className="w-36"
              options={IDLE_CLOSE_OPTIONS}
              value={
                IDLE_CLOSE_OPTIONS.some((o) => o.value === String(browserIdleCloseMin))
                  ? String(browserIdleCloseMin)
                  : "10"
              }
              onChange={(v) => void changeBrowserIdleCloseMin(Number(v))}
            />
          </SettingItem>
        )}
      </div>
    </section>
  );
}
