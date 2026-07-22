import type { LucideIcon } from "lucide-react";
import {
  Accessibility,
  Bell,
  CalendarDays,
  HardDrive,
  ListChecks,
  Workflow,
} from "lucide-react";
import type { PermissionKind } from "../../../api";

export const systemPermissionsCopy = {
  sectionTitle: "系统授权",
  sectionDesc: "在电脑上运行所需要的系统授权",
  granted: "已授权",
  denied: "未授权",
  authorize: "去授权",
  openSettings: "去设置",
  recheck: "重新检测",
  relaunchHint: "授权后需重启应用才能生效",
  relaunchNow: "立即重启",
} as const;

export interface PermissionRowConfig {
  kind: PermissionKind;
  icon: LucideIcon;
  title: string;
  description: string;
}

// 顺序与后端 PANEL_KINDS 一致（完全磁盘 / 辅助功能 / 自动化 / 日历 / 提醒 / 通知）。
export const PERMISSION_ROWS: PermissionRowConfig[] = [
  {
    kind: "full_disk",
    icon: HardDrive,
    title: "完全磁盘访问权限",
    description: "允许访问磁盘上的所有文件，部分功能需要此权限才能正常工作",
  },
  {
    kind: "accessibility",
    icon: Accessibility,
    title: "辅助功能",
    description: "允许响应键盘快捷键，便于快捷唤起等功能",
  },
  {
    kind: "automation",
    icon: Workflow,
    title: "自动化",
    description: "允许给备忘录等 App 发指令，帮你读写笔记",
  },
  {
    kind: "calendars",
    icon: CalendarDays,
    title: "日历",
    description: "允许读取和管理你的日历事件",
  },
  {
    kind: "reminders",
    icon: ListChecks,
    title: "提醒事项",
    description: "允许读取和管理你的提醒事项",
  },
  {
    kind: "notification",
    icon: Bell,
    title: "通知",
    description: "允许发送桌面通知，任务完成或有新消息时会及时提醒你",
  },
];
