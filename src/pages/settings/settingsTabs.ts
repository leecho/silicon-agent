export type SettingsTabId =
  | "preferences"
  | "permissions"
  | "automation"
  | "model-provider"
  | "model-advance"
  | "memory"
  | "knowledge-base"
  | "usage-analysis"
  | "call-log";

export interface SettingsTabItem {
  id: SettingsTabId;
  label: string;
  description: string;
  group: string;
}

export const settingsTabs: SettingsTabItem[] = [
  {
    id: "preferences",
    label: "常规",
    description: "主题、界面显示与快捷建议",
    group: "通用",
  },
  {
    id: "permissions",
    label: "权限",
    description: "默认权限模式与系统授权",
    group: "通用",
  },
  {
    id: "automation",
    label: "自动化",
    description: "桌面操作、浏览器操作与静默模式",
    group: "通用",
  },
  {
    id: "model-provider",
    label: "模型配置",
    description: "厂商与密钥、全局默认模型、备用与辅助模型",
    group: "模型",
  },
  {
    id: "model-advance",
    label: "运行设置",
    description: "助手如何推进任务：迭代、上下文压缩、子代理与失败重试",
    group: "模型",
  },
  {
    id: "memory",
    label: "记忆",
    description: "查看与删除模型记录的长期记忆",
    group: "数据与诊断",
  },
  {
    id: "knowledge-base",
    label: "知识库",
    description: "智能查找（向量检索）与向量模型",
    group: "数据与诊断",
  },
  {
    id: "usage-analysis",
    label: "用量",
    description: "Token 用量与缓存统计（采集自启用后产生的运行）",
    group: "数据与诊断",
  },
  {
    id: "call-log",
    label: "调用日志",
    description: "记录每次模型调用的完整请求与响应，供调试/审计（默认关闭）",
    group: "数据与诊断",
  },
];

export function getSettingsTab(id: SettingsTabId): SettingsTabItem {
  return settingsTabs.find((tab) => tab.id === id) ?? settingsTabs[0];
}
