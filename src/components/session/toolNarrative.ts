// 工具叙事标签生成：按 toolName + 状态 + 关键参数生成自然语言短句，
// 替代「web_search {json}」式原始展示（原始 input/output 收进展开详情）。

const ACTION_BASE: Record<string, string> = {
  web_search: "搜索网页",
  web_fetch: "抓取网页",
  read_file: "读取文件",
  write_file: "写入文件",
  edit_file: "编辑文件",
  glob: "查找文件",
  grep: "搜索内容",
  run_command: "执行命令",
  load_skill: "加载技能",
  read_skill_file: "读取技能文件",
  install_skill: "安装技能",
  update_todos: "更新待办",
  add_artifact: "登记产物",
  ask_user: "向用户提问",
  propose_plan: "提交计划",
};

const ARG_KEYS: Record<string, string[]> = {
  web_search: ["query"],
  web_fetch: ["url"],
  read_file: ["path"],
  write_file: ["path"],
  edit_file: ["path"],
  glob: ["pattern"],
  grep: ["pattern"],
  run_command: ["command", "program"],
  load_skill: ["name"],
  read_skill_file: ["rel_path", "path"],
  install_skill: ["skill_path"],
  add_artifact: ["title", "path"],
  propose_plan: ["title"],
};

/**
 * 从工具行 input 取 `name` 字段。先按完整 JSON 解析；失败（生成期流式半截 JSON）则正则兜底——
 * 只要 name 值已吐完（收尾引号到了）即可提取，无需整段 JSON 闭合。name 尚未流到时返回空串。
 */
export function parseDispatchName(input: string): string {
  try {
    const v = JSON.parse(input || "{}").name;
    if (typeof v === "string" && v) return v;
  } catch {
    // 流式半截 JSON：下面正则兜底。
  }
  const m = input.match(/"name"\s*:\s*"((?:[^"\\]|\\.)*)"/);
  return m ? m[1] : "";
}

function truncateFragment(value: string): string {
  return value.length > 50 ? value.slice(0, 50) + "..." : value;
}

function firstString(args: Record<string, unknown>, keys: string[]): string {
  for (const key of keys) {
    const value = args[key];
    if (typeof value === "string" && value) return truncateFragment(value);
  }
  return "";
}

function arrayCount(value: unknown, unit: string): string {
  return Array.isArray(value) ? `${value.length} ${unit}` : "";
}

function firstQuestion(value: unknown): string {
  if (!Array.isArray(value)) return "";
  const first = value[0];
  if (!first || typeof first !== "object") return "";
  const question = (first as Record<string, unknown>).question;
  if (typeof question === "string" && question) return truncateFragment(question);
  return arrayCount(value, "个问题");
}

function toolArgumentFragment(toolName: string, args: Record<string, unknown>): string {
  if (toolName === "ask_user") return firstQuestion(args.questions);
  if (toolName === "update_todos") return arrayCount(args.todos, "项待办");
  const keys = ARG_KEYS[toolName];
  return keys ? firstString(args, keys) : "";
}

export function toolNarrative(
  toolName: string,
  status: string,
  inputJson: string,
  /** agent name → 展示名映射（如把 image-creator 显示为 珀西）；缺省用原始 name。 */
  agentDisplayNames?: Record<string, string>,
): string {
  // 子运行（child agent）单独叙事：以专家名为主语、按状态描述「准备/处理/回禀」，更贴近用户视角。
  if (toolName === "dispatch_agent") {
    const raw = parseDispatchName(inputJson);
    const name = raw ? (agentDisplayNames?.[raw] ?? raw) : raw;
    const who = name ? `专家「${name}」` : "专家";
    if (status === "generating") return `${who} 准备中`;
    if (status === "running") return `${who} 处理中`;
    if (status === "failed") return `${who} 处理失败`;
    return `${who} 已回复`;
  }
  const base = ACTION_BASE[toolName] ?? `调用 ${toolName}`;
  let frag = "";
  try {
    const args = JSON.parse(inputJson || "{}");
    if (args && typeof args === "object" && !Array.isArray(args)) {
      frag = toolArgumentFragment(toolName, args as Record<string, unknown>);
    }
  } catch {
    // 忽略无法解析的工具参数，保持叙事标签可用。
  }
  const prefix =
    status === "generating"
      ? `正在准备${base}...`
      : status === "running"
        ? `正在${base}...`
        : status === "failed"
          ? `${base}失败`
          : `已${base}`;
  return frag ? `${prefix} ${frag}` : prefix;
}

export function getToolNarrative(toolName: string): string {
  return ACTION_BASE[toolName] ?? toolName;
}

export function toolActivityLabel(
  toolName: string | undefined,
  status?: string,
): string {
  const name = toolName || "工具";
  const base = ACTION_BASE[name] ?? `调用 ${name}`;
  if (status === "generating") return `正在准备${base}`;
  return `正在${base}`;
}
