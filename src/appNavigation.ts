import {
  Home,
  MessagesSquare,
  Wrench,
  MessageCircleMore,
  type LucideIcon,
} from "lucide-react";

export type AppSection =
  | "home"
  | "session"
  | "skills"
  | "remote"
  | "settings";

export interface NavItem {
  id: AppSection;
  label: string;
  icon: LucideIcon;
}

export const primaryNavItems: NavItem[] = [
  { id: "home", label: "首页", icon: Home },
  { id: "session", label: "会话", icon: MessagesSquare },
  { id: "skills", label: "技能", icon: Wrench },
  { id: "remote", label: "IM渠道", icon: MessageCircleMore },
];
