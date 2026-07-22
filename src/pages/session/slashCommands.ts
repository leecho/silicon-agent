export interface SlashCommand {
  name: string;
  usage: string;
  description: string;
}

export const SLASH_COMMANDS: SlashCommand[] = [
  { name: "/new", usage: "/new", description: "新建会话" },
  { name: "/clear", usage: "/clear", description: "清空当前会话显示(不删除历史)" },
  { name: "/rename", usage: "/rename [新名称]", description: "重命名当前会话" },
  { name: "/stop", usage: "/stop", description: "停止当前运行" },
  { name: "/memory", usage: "/memory", description: "查看长期记忆" },
  { name: "/compact", usage: "/compact", description: "压缩较早对话以省上下文" },
  { name: "/plan", usage: "/plan", description: "开启/关闭计划模式" },
  { name: "/help", usage: "/help", description: "显示可用命令" },
];

export interface ParsedCommand {
  name: string;
  args: string[];
  command?: SlashCommand;
  raw: string;
}

export function parseSlashCommand(input: string): ParsedCommand | null {
  const t = input.trim();
  if (!t.startsWith("/")) return null;
  const [name, ...args] = t.split(/\s+/);
  const lower = name.toLowerCase();
  return {
    name: lower,
    args,
    command: SLASH_COMMANDS.find((c) => c.name === lower),
    raw: t,
  };
}

// 菜单过滤：仅以 / 开头、且未输入完整命令+空格时给建议。
export function slashSuggestions(input: string): SlashCommand[] {
  if (!input.startsWith("/")) return [];
  if (/^\/\S+\s/.test(input)) return [];
  const q = input.slice(1).split(/\s+/)[0].toLowerCase();
  return SLASH_COMMANDS.filter(
    (c) =>
      !q ||
      c.name.slice(1).includes(q) ||
      c.description.toLowerCase().includes(q),
  );
}
