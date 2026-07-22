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

// 过程区内的时间线项：思考 / 旁白（中间 assistant 文本）/ 工具。
export type ProcessItem =
  | { kind: "thinking"; id: string; text: string }
  | { kind: "narration"; id: string; text: string }
  | Extract<FeedRow, { kind: "tool" }>;

// 渲染期分组行：assistant/tool 被聚合；其余行原样透传。
// processGroup = 一轮内除最终答案外的思考/旁白/工具；answer = 最终答案（常驻）。
export type GroupedFeedRow =
  | Exclude<FeedRow, { kind: "tool" | "assistant" }>
  | { kind: "answer"; id: string; reasoning?: string; content: string }
  | { kind: "processGroup"; id: string; items: ProcessItem[] };

// 轮感知分组：以 user/divider/error/askAnswer 为边界把 {assistant,tool} 聚成 bucket，
// 再拆成 [过程区, 最终答案]。最终答案 = bucket 末条「非空 content 的 assistant」；
// 其余（更早 reasoning + 旁白 + 全部工具）摊平进过程区；过程区为空则不产出。
export function groupRows(rows: FeedRow[]): GroupedFeedRow[] {
  const out: GroupedFeedRow[] = [];
  let bucket: Array<Extract<FeedRow, { kind: "assistant" | "tool" }>> = [];

  const flush = () => {
    if (bucket.length === 0) return;

    // 最终答案：bucket 最后一条且为非空 content 的 assistant（一轮正常以答案收尾）。
    // 末条是工具或空 content assistant（运行中/以工具收尾）→ 无最终答案，全进过程区。
    const last = bucket[bucket.length - 1];
    const hasAnswer = last.kind === "assistant" && last.content.length > 0;
    const answerIdx = hasAnswer ? bucket.length - 1 : -1;

    const items: ProcessItem[] = [];
    for (let i = 0; i < bucket.length; i++) {
      if (i === answerIdx) continue; // 最终答案的 content 不进过程区（其 reasoning 见下）
      const r = bucket[i];
      if (r.kind === "tool") {
        items.push(r);
      } else {
        if (r.reasoning && r.reasoning.length > 0) {
          items.push({ kind: "thinking", id: r.id, text: r.reasoning });
        }
        if (r.content.length > 0) {
          items.push({ kind: "narration", id: r.id, text: r.content });
        }
      }
    }

    const answer =
      answerIdx >= 0
        ? (bucket[answerIdx] as Extract<FeedRow, { kind: "assistant" }>)
        : null;

    // 最终答案的思考：若本轮已有过程内容，则并入过程区末尾（避免答案上方悬一个孤立
    // 「深度思考」折叠）；否则（无工具无旁白的简单轮）保留为答案自带的折叠行。
    let answerReasoning = answer?.reasoning;
    if (answer && items.length > 0 && answer.reasoning && answer.reasoning.length > 0) {
      items.push({ kind: "thinking", id: answer.id, text: answer.reasoning });
      answerReasoning = undefined;
    }

    if (items.length > 0) {
      out.push({ kind: "processGroup", id: "p:" + bucket[0].id, items });
    }
    if (answer) {
      out.push({
        kind: "answer",
        id: answer.id,
        reasoning: answerReasoning,
        content: answer.content,
      });
    }

    bucket = [];
  };

  for (const row of rows) {
    if (row.kind === "tool" || row.kind === "assistant") {
      bucket.push(row);
    } else {
      flush();
      out.push(row);
    }
  }
  flush();
  return out;
}
