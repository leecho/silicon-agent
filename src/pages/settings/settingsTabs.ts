export type SettingsTabId =
  | "model-advance"
  | "model-provider"
  | "usage-analysis"
  | "call-log"
  | "preferences"
  | "agent-persona";

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
    description: "主题与新会话默认权限",
    group: "基础",
  },
  {
    id: "agent-persona",
    label: "人设",
    description: "自定义 Agent 的身份与灵魂（留空则用默认人设）",
    group: "基础",
  },
  {
    id: "model-advance",
    label: "高级配置",
    description: "模型运行行为、备用模型与辅助模型",
    group: "模型",
  },
  {
    id: "model-provider",
    label: "模型配置",
    description: "配置 OpenAI 兼容接口与密钥",
    group: "模型",
  },
  {
    id: "usage-analysis",
    label: "用量分析",
    description: "Token 用量与缓存统计（采集自启用后产生的运行）",
    group: "模型",
  },
  {
    id: "call-log",
    label: "调用日志",
    description: "记录每次模型调用的完整请求与响应，供调试/审计（默认关闭）",
    group: "模型",
  },
];

export function getSettingsTab(id: SettingsTabId): SettingsTabItem {
  return settingsTabs.find((tab) => tab.id === id) ?? settingsTabs[0];
}
