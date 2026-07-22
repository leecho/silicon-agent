import { Blocks, Cable, GraduationCap, Store, Users, Wrench, type LucideIcon } from "lucide-react";

/**
 * 「扩展」页的胶囊 Tab（T106 §5.2）。市场为落地页。
 *
 * 团队也在这里——但那只是**导航形态**：运行时它仍是编排层（角色槽 + lead SOP + roster），
 * 不是一条全局能力。市场里的团队以「带 teamInfo 的能力包」发货，装完落到本页团队 Tab
 * （T108 三体系：插件公开、专家/团队私有按需载入）。
 *
 * 资料库不在这里（知识入口，独立侧栏项）。
 *
 * 图标：插件=Blocks、技能=Wrench、专家=GraduationCap（专长）、团队=Users（人群）、MCP=Cable。
 * 专家不用 Bot——那与「智能体」的机器人语义撞车。
 */
export type ExtensionTabId = "market" | "plugins" | "skills" | "experts" | "teams" | "mcp";

export const EXTENSION_TABS: { id: ExtensionTabId; label: string; icon: LucideIcon }[] = [
  { id: "market", label: "市场", icon: Store },
  { id: "plugins", label: "插件", icon: Blocks },
  { id: "skills", label: "技能", icon: Wrench },
  { id: "experts", label: "专家", icon: GraduationCap },
  { id: "teams", label: "团队", icon: Users },
  { id: "mcp", label: "MCP", icon: Cable },
];

export const DEFAULT_EXTENSION_TAB: ExtensionTabId = "market";
