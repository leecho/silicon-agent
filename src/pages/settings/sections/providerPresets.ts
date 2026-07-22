import type { ProviderProtocol } from "../../../types";

/** 预置厂商模板：选中后自动填 name + baseUrl；用户仍可改。 */
export interface ProviderPreset {
  key: string;
  name: string;
  baseUrl: string;
  protocol: ProviderProtocol;
  defaultModels?: string[];
}

export const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    key: "deepseek",
    name: "DeepSeek",
    baseUrl: "https://api.deepseek.com/v1",
    protocol: "openai",
    defaultModels: ["deepseek-chat", "deepseek-reasoner"],
  },
  {
    key: "anthropic",
    name: "Anthropic",
    baseUrl: "https://api.anthropic.com",
    protocol: "anthropic",
    defaultModels: ["claude-opus-4-8", "claude-sonnet-4-6"],
  },
  {
    key: "openai",
    name: "OpenAI",
    baseUrl: "https://api.openai.com/v1",
    protocol: "openai",
    defaultModels: ["gpt-4.1", "gpt-4.1-mini", "gpt-4o"],
  },
  {
    key: "dashscope",
    name: "阿里百炼",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    protocol: "openai",
    defaultModels: ["qwen-plus", "qwen-max", "qwen-turbo"],
  },
  {
    key: "moonshot",
    name: "Moonshot",
    baseUrl: "https://api.moonshot.cn/v1",
    protocol: "openai",
    defaultModels: ["moonshot-v1-8k", "moonshot-v1-32k", "moonshot-v1-128k"],
  },
  {
    key: "siliconflow",
    name: "SiliconFlow",
    baseUrl: "https://api.siliconflow.cn/v1",
    protocol: "openai",
    defaultModels: ["deepseek-ai/DeepSeek-V3", "Qwen/Qwen2.5-72B-Instruct"],
  },
  { key: "custom", name: "自定义", baseUrl: "", protocol: "openai" },
];
