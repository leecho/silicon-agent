// 工具叙事标签生成：按 toolName + 状态 + 关键参数生成自然语言短句，
// 替代「web_search {json}」式原始展示（原始 input/output 收进展开详情）。

// 工具中文标签的单一真相源 = 后端 Tool::label()，经 get_tool_labels 命令注入（见 setToolLabels）。
// 前端不再硬编码标签表：新增工具只需后端覆写 label()，叙事自动跟随，杜绝两处漂移与原始英文名泄漏。
let toolLabels: Record<string, string> = {};

/** 由 App 启动时调 getToolLabels() 注入；单一来源 = 后端 Tool::label()。 */
export function setToolLabels(map: Record<string, string>): void {
  toolLabels = map;
}

// 历史会话兼容：这些工具已更名（create_expert→install_expert、create_team→install_team，T107），
// 后端 registry 不再发布旧名的 label，但老会话里仍存着旧名的 tool_call。
// 仅当后端 toolLabels 无此名时回退到这里，保证历史叙事仍显示中文动作名而非泛化的「执行操作」。
const LEGACY_TOOL_LABELS: Record<string, string> = {
  create_expert: "创建专家",
  create_team: "创建团队",
};

function labelOf(toolName: string): string | undefined {
  return toolLabels[toolName] ?? LEGACY_TOOL_LABELS[toolName];
}

// 浏览器工具按本次 action 叙事「具体动作」（打开网页/点击/填写…），而非笼统「浏览器操作」
// （「已浏览器操作」动宾不通顺）。用户视角动词，与 BrowserPanel 的 copy.ts 同口径。
const BROWSER_ACTION_VERBS: Record<string, string> = {
  navigate: "打开网页",
  observe: "查看页面",
  click: "点击",
  double_click: "双击",
  fill: "填写",
  select: "选择",
  scroll: "滚动",
  extract: "提取内容",
  wait: "等待",
  back: "返回上一页",
  tabs: "查看标签页",
  switch_tab: "切换标签页",
  close_tab: "关闭标签页",
  close: "关闭浏览器",
};

// 桌面操作（computer）同理：按 action 叙事具体动作，替代笼统「桌面操作」（「已桌面操作」不通顺）。
// 用户视角动词，与 ComputerActionStream 的 copy.ts 同口径。
const COMPUTER_ACTION_VERBS: Record<string, string> = {
  observe: "查看屏幕",
  click: "点击",
  double_click: "双击",
  type: "输入",
  key: "按键",
  scroll: "滚动",
  wait: "等待",
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
  install_skill: ["skill_path"],
  install_plugin: ["plugin_path"],
  install_expert: ["display_name", "name"],
  install_team: ["display_name", "name"],
  // 旧工具名（create_expert/create_team 已改名为 install_expert/install_team），
  // 保留供历史会话中已落盘的 tool_call 重渲染时仍能取到叙事细节，不可删除。
  create_expert: ["display_name", "name"],
  create_team: ["display_name", "name"],
  update_tasks: ["goal"],
  add_artifact: ["title", "path"],
  remember: ["content"],
  propose_plan: ["title"],
  dispatch_agent: ["name"],
};

/**
 * 从 dispatch_agent 的 input 取专家名。先按完整 JSON 解析；失败（生成期流式半截 JSON）则正则兜底——
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

/**
 * 解析 MCP 代理工具的注册名 `mcp__{服务}__{功能}`（后端 McpToolProxy 生成）。
 * 服务段是后端 slug（小写 ASCII），功能段下划线转空格便于阅读。
 * 非 MCP 工具返回 null，走通用叙事。
 */
export function parseMcpToolName(
  toolName: string,
): { server: string; func: string } | null {
  const m = toolName.match(/^mcp__(.+?)__(.+)$/);
  if (!m) return null;
  return { server: m[1], func: m[2].replace(/_/g, " ") };
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
  if (typeof question === "string" && question)
    return truncateFragment(question);
  return arrayCount(value, "个问题");
}

function collectAgentsFragment(args: Record<string, unknown>): string {
  const handles = args.handles;
  if (Array.isArray(handles) && handles.length > 0)
    return `${handles.length} 个专家`;
  return "全部后台专家";
}

function toolArgumentFragment(
  toolName: string,
  args: Record<string, unknown>,
): string {
  if (toolName === "ask_user") return firstQuestion(args.questions);
  if (toolName === "update_todos") return arrayCount(args.todos, "项待办");
  if (toolName === "update_tasks") {
    return (
      firstString(args, ARG_KEYS.update_tasks) ||
      arrayCount(args.tasks, "项任务")
    );
  }
  if (toolName === "collect_agents") return collectAgentsFragment(args);
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
  // 指派专家单独叙事：以专家名为主语、按状态描述「准备/处理/回禀」，更贴近用户视角。
  if (toolName === "dispatch_agent") {
    const raw = parseDispatchName(inputJson);
    const name = raw ? (agentDisplayNames?.[raw] ?? raw) : raw;
    const who = name ? `专家「${name}」` : "专家";
    if (status === "generating") return `${who} 准备中`;
    if (status === "running") return `${who} 处理中`;
    if (status === "failed") return `${who} 处理失败`;
    return `${who} 已回复`;
  }
  // MCP 外部工具：注册名 mcp__{服务}__{功能} 渲染为「MCP·服务 · 功能」徽标式，
  // 替代原始 mcp__... 串，并显式标出 MCP 来源（外部服务，符合可审计边界原则）。
  const mcp = parseMcpToolName(toolName);
  if (mcp) {
    const target = `MCP·${mcp.server} · ${mcp.func}`;
    if (status === "generating") return `正在准备调用 ${target}`;
    if (status === "running") return `正在调用 ${target}`;
    if (status === "failed") return `调用 ${target} 失败`;
    return `已调用 ${target}`;
  }
  // 浏览器/桌面工具：按本次 action 叙事具体动作（打开网页/查看屏幕/点击/输入…），
  // 替代笼统又不通顺的「已浏览器操作」「已桌面操作」。
  const actionVerbs =
    toolName === "browser"
      ? BROWSER_ACTION_VERBS
      : toolName === "computer"
        ? COMPUTER_ACTION_VERBS
        : null;
  if (actionVerbs) {
    let action = "";
    try {
      const a = JSON.parse(inputJson || "{}");
      if (a && typeof a === "object") action = String((a as Record<string, unknown>).action ?? "");
    } catch {
      // 参数未解析完（流式中）时退到通用动词。
    }
    const fallback = toolName === "browser" ? "操作浏览器" : "操作桌面";
    const verb = actionVerbs[action] ?? fallback;
    if (status === "generating") return `正在准备${verb}...`;
    if (status === "running") return `正在${verb}...`;
    if (status === "failed") return `${verb}失败`;
    return `已${verb}`;
  }
  // 缺标签时退到通用动作词（绝不回退到原始英文工具名，避免如「调用 computer」泄漏）。
  const base = labelOf(toolName) ?? "执行操作";
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
  const mcp = parseMcpToolName(toolName);
  if (mcp) return `MCP·${mcp.server} · ${mcp.func}`;
  return labelOf(toolName) ?? "操作";
}

export function toolActivityLabel(
  toolName: string | undefined,
  status?: string,
): string {
  const name = toolName || "工具";
  const mcp = parseMcpToolName(name);
  const base = mcp
    ? `调用 MCP·${mcp.server}·${mcp.func}`
    : (labelOf(name) ?? "执行操作");
  if (status === "generating") return `正在准备${base}`;
  return `正在${base}`;
}
