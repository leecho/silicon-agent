import type { McpServerConfig, McpServerStatus } from "../../api";

/**
 * MCP 的「人话层」：把协议/传输/后端错误翻译成普通用户看得懂的话。
 *
 * 这一页面向的是**装了插件想用起来的普通人**，不是写 MCP server 的开发者。
 * 他们不需要知道 stdio/SSE、DCR、client_id 是什么——只需要知道
 * 「能不能用」「要我干什么」。原始技术信息不删除，收进各处的「详细」折叠里备查。
 */

type State = McpServerStatus["state"];

/** 状态短语：站在用户视角说「这个服务现在能不能用」，而不是连接的内部状态机名。 */
export const STATE_LABEL: Record<State, string> = {
  connected: "可用",
  connecting: "连接中",
  unauthorized: "需要登录",
  failed: "连接失败",
  disconnected: "已停用",
};

export const STATE_DOT: Record<State, string> = {
  connected: "bg-success",
  connecting: "bg-warning",
  unauthorized: "bg-warning",
  failed: "bg-destructive",
  disconnected: "bg-foreground-muted",
};

/** 状态文字的着色：只有真正需要用户动手的状态才染色，避免一片红把人吓住。 */
export const STATE_TONE: Record<State, string> = {
  connected: "text-success",
  connecting: "text-foreground-muted",
  unauthorized: "text-warning",
  failed: "text-destructive",
  disconnected: "text-foreground-muted",
};

/**
 * 服务来自哪里：远程只给域名（`mcp.figma.com`），本地只说「本机运行」。
 *
 * 刻意**不显示** HTTP/SSE/stdio——传输协议是实现细节，用户拿它做不了任何决定。
 * 要改地址走「编辑配置」，那里是完整的 JSON。
 */
export function originLabel(s: McpServerConfig): string {
  if (s.transport.type === "stdio") return "本机运行";
  try {
    return new URL(s.transport.url).host;
  } catch {
    return s.transport.url;
  }
}

/**
 * 这条错误是不是「服务不支持自动注册、必须手填应用 ID」。
 *
 * 唯一需要把 `client_id` 摆到用户面前的场景——其余情况它不该出现在界面上。
 */
export function needsClientId(raw: string | null | undefined): boolean {
  if (!raw) return false;
  const e = raw.toLowerCase();
  return raw.includes("动态注册") || e.includes("registration_endpoint") || e.includes("dcr");
}

/** 一条给用户的错误：`title` 是人话结论，`hint` 是下一步该干什么。 */
export interface FriendlyError {
  hint?: string;
  title: string;
}

/**
 * 把后端错误翻成用户看得懂的话。
 *
 * 后端错误是给日志看的（`[unauthorized] 401 Unauthorized` 这种），直接摔到用户脸上
 * 只会让人放弃。这里按**用户能做什么**来分类——每一类都要能回答「那我该怎么办」。
 *
 * 文案取**陈述句、无口语**：说清结论和下一步即可，不寒暄、不解释原理。
 * 匹配不上的兜底为「该服务当前不可用」，原文仍在错误卡片的「详细」折叠里。
 */
export function friendlyError(raw: string): FriendlyError {
  const e = raw.toLowerCase();

  if (e.includes("动态注册") || e.includes("registration_endpoint") || e.includes("dcr")) {
    return {
      title: "该服务不支持自动登录",
      hint: "需在服务方的开发者后台申请应用 ID，并填入下方「应用 ID」。",
    };
  }
  if (e.includes("unauthorized") || e.includes("401") || e.includes("403")) {
    return { title: "登录已过期", hint: "请重新登录。" };
  }
  if (
    e.includes("dns") ||
    e.includes("timed out") ||
    e.includes("timeout") ||
    e.includes("connection refused") ||
    e.includes("network")
  ) {
    return { title: "无法连接到该服务", hint: "请检查网络后重试。" };
  }
  if (
    e.includes("no such file") ||
    e.includes("not found") ||
    e.includes("enoent") ||
    e.includes("program not found")
  ) {
    return {
      title: "缺少所需的本地程序",
      hint: "该服务需在本机运行一个命令，但未找到该命令。",
    };
  }
  if (e.includes("json") || e.includes("parse") || e.includes("invalid")) {
    return { title: "服务配置有误", hint: "请检查服务地址与参数。" };
  }
  return { title: "该服务当前不可用", hint: "请重试，或展开「详细」查看原因。" };
}

/**
 * 服务名去掉包管理器风格的前缀（`@modelcontextprotocol/server-filesystem` → `filesystem`）。
 * 展示用；真实 id/name 不变。
 */
export function displayName(name: string): string {
  const tail = name.split("/").pop() ?? name;
  return tail.replace(/^(mcp-|server-)/, "").replace(/-(mcp|server)$/, "");
}

/** 工具名 → 人话（`get_file_content` → `Get file content`）。description 为空时的兜底。 */
export function humanizeToolName(name: string): string {
  const words = name.replace(/[_-]+/g, " ").replace(/([a-z])([A-Z])/g, "$1 $2").trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}
