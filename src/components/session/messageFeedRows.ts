import type { FeedRow, Message } from "../../types";

function parseMessageTimestamp(value?: string | null): number | null {
  if (!value) return null;
  const seconds = Number(value);
  if (Number.isFinite(seconds) && seconds > 0) return seconds * 1000;
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : null;
}

/**
 * 从持久消息构建统一 FeedRow 列表。
 * 先扫 assistant 的 toolCallsJson 建立 callId -> 输入参数 的映射，
 * 再遍历 messages 产出 user/assistant/tool 行（tool 行 output=content、input 来自映射）。
 */
export function buildPersistedRows(messages: Message[], showProcess = true): FeedRow[] {
  const inputByCallId = new Map<string, string>();
  const startedAtByCallId = new Map<string, number>();
  for (const m of messages) {
    if (m.role === "assistant" && m.toolCallsJson) {
      try {
        // ModelToolCall 序列化为 camelCase：{ id, name, argumentsJson }。
        const calls = JSON.parse(m.toolCallsJson) as Array<{
          id?: string;
          name?: string;
          argumentsJson?: string;
        }>;
        const startedAt = parseMessageTimestamp(m.createdAt);
        for (const c of calls) {
          if (c.id) {
            inputByCallId.set(c.id, c.argumentsJson ?? "");
            if (startedAt !== null) startedAtByCallId.set(c.id, startedAt);
          }
        }
      } catch {
        // 忽略损坏的 toolCallsJson。
      }
    }
  }

  const rows: FeedRow[] = [];
  for (const m of messages) {
    if (m.role === "user") {
      rows.push({ kind: "user", id: m.id, content: m.content });
    } else if (m.role === "compaction" || m.role === "stopped") {
      rows.push({ kind: "divider", id: m.id, content: m.content });
    } else if (m.role === "error") {
      rows.push({ kind: "error", id: m.id, content: m.content });
    } else if (m.role === "assistant") {
      if (m.content.length > 0 || (showProcess && m.reasoning && m.reasoning.length > 0)) {
        rows.push({
          kind: "assistant",
          id: m.id,
          reasoning: showProcess ? m.reasoning : undefined,
          content: m.content,
        });
      }
    } else if (m.role === "tool" && m.toolName === "ask_user") {
      // ask_user 的工具结果即「用户的回答」（或取消标记）：始终展示便于追溯，
      // 不受「显示完成过程」开关影响，也不并入工具步骤折叠组。
      // 取消由结构化的 toolStatus="cancelled" 判别（非正文嗅探），渲染为分隔线。
      if (m.toolStatus === "cancelled") {
        rows.push({ kind: "divider", id: m.id, content: "已取消提问" });
      } else {
        rows.push({ kind: "askAnswer", id: m.id, content: m.content });
      }
    } else if (showProcess && m.role === "tool") {
      rows.push({
        kind: "tool",
        id: m.id,
        toolCallId: m.toolCallId ?? undefined,
        toolName: m.toolName ?? "工具",
        input: m.toolCallId ? (inputByCallId.get(m.toolCallId) ?? "") : "",
        output: m.content,
        startedAt: m.toolCallId ? startedAtByCallId.get(m.toolCallId) : undefined,
        finishedAt: parseMessageTimestamp(m.createdAt) ?? undefined,
        status: m.toolStatus === "failed" ? "failed" : "done",
      });
    }
  }
  return rows;
}

// 渲染期分组：把连续的 tool 行合并成一个 toolGroup，遇非 tool 行断组（单条 tool 也成组）。
export type GroupedFeedRow =
  | Exclude<FeedRow, { kind: "tool" }>
  | {
      kind: "toolGroup";
      id: string;
      steps: Array<Extract<FeedRow, { kind: "tool" }>>;
    };

export function groupRows(rows: FeedRow[]): GroupedFeedRow[] {
  const out: GroupedFeedRow[] = [];
  let bucket: Array<Extract<FeedRow, { kind: "tool" }>> = [];
  const flush = () => {
    if (bucket.length > 0) {
      out.push({ kind: "toolGroup", id: "g:" + bucket[0].id, steps: bucket });
      bucket = [];
    }
  };
  for (const row of rows) {
    if (row.kind === "tool") {
      bucket.push(row);
    } else {
      flush();
      out.push(row);
    }
  }
  flush();
  return out;
}
