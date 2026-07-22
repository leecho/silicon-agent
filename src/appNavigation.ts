import {
  Blocks,
  BookMarked,
  Bot,
  FolderKanban,
  Home,
  MessagesSquare,
  Timer,
  type LucideIcon,
} from "lucide-react";

export type AppSection =
  | "home"
  | "session"
  | "extensions"
  | "knowledge-bases"
  | "agents"
  | "projects"
  | "scheduling"
  | "remote"
  | "settings";

export interface NavItem {
  id: AppSection;
  label: string;
  icon: LucideIcon;
}

/**
 * T106：原「技能 / 套件 / 资料库(留) / 连接器 / 专家」中的四项（技能/套件/连接器/专家）
 * 合并为单入口「扩展」（内部胶囊 Tab：市场/插件/技能/专家/团队/MCP）。
 * 「资料库」保持独立（知识入口）。
 * 团队并入「扩展」只是导航形态——运行时它仍是编排层（成员与技能为其私有，激活时才载入）。
 */
export const primaryNavItems: NavItem[] = [
  { id: "home", label: "首页", icon: Home },
  { id: "session", label: "会话", icon: MessagesSquare },
  { id: "extensions", label: "能力中心", icon: Blocks },
  { id: "knowledge-bases", label: "资料库", icon: BookMarked },
  { id: "scheduling", label: "定时任务", icon: Timer },
];
