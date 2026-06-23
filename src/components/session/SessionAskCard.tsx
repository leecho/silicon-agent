import { useState } from "react";
import { X } from "lucide-react";
import { Button } from "../ui/Button";
import { MarkdownText } from "../ui/MarkdownText";
import { Tooltip } from "../ui/Tooltip";
import type { PendingAsk } from "../../types";

// 判断某选项是否本质上等价于「其他/自由作答」——这类选项与固定的自由输入框重复，
// 模型偶尔会把它写进 options（尽管提示要求不要），渲染时滤掉，只保留下方输入框。
function isOtherLikeOption(opt: string): boolean {
  const s = opt
    .trim()
    .toLowerCase()
    .replace(/[（(][^）)]*[)）]\s*$/, "") // 去掉结尾括注，如「其他（请说明）」
    .replace(/[:：…\.\s]+$/, "")
    .trim();
  return [
    "其他",
    "其它",
    "别的",
    "自定义",
    "自由输入",
    "自由作答",
    "other",
    "others",
  ].includes(s);
}

// 模型 ask_user 反问卡：多问题分页作答（主题 + 单/多选 + 可跳过）。
export function SessionAskCard({
  ask,
  busy,
  onAnswer,
  onCancel,
  agentLabel,
}: {
  ask: PendingAsk;
  busy: boolean;
  onAnswer: (answers: string[][]) => void;
  /** 取消回答并立即停止会话（卡片右上角 ✕）。 */
  onCancel?: () => void;
  /** 子代理提问冒泡时，标注是哪个专家在提问。 */
  agentLabel?: string;
}) {
  const questions = ask.questions ?? [];
  const total = questions.length;
  const [idx, setIdx] = useState(0);
  const [selected, setSelected] = useState<string[][]>(() =>
    questions.map(() => []),
  );
  const [freeText, setFreeText] = useState<string[]>(() =>
    questions.map(() => ""),
  );

  if (total === 0) {
    return (
      <div className="m-3 shrink-0 rounded-lg border border-border-subtle bg-card px-4 py-3 shadow-sm">
        <div className="mb-2 text-sm text-foreground-secondary">无待回答问题。</div>
        <div className="flex justify-end gap-2">
          {onCancel && (
            <Button disabled={busy} onClick={onCancel}>
              取消并停止
            </Button>
          )}
          <Button tone="primary" disabled={busy} onClick={() => onAnswer([])}>
            继续
          </Button>
        </div>
      </div>
    );
  }

  const q = questions[idx];
  const isLast = idx === total - 1;
  // 滤掉与固定输入框重复的「其他」类选项；自由作答统一交给下方输入框。
  const visibleOptions = q.options.filter((opt) => !isOtherLikeOption(opt));

  const toggle = (opt: string) => {
    setSelected((prev) => {
      const next = prev.map((a) => [...a]);
      const cur = next[idx];
      if (q.multiSelect) {
        const at = cur.indexOf(opt);
        if (at >= 0) cur.splice(at, 1);
        else cur.push(opt);
      } else {
        next[idx] = cur.length === 1 && cur[0] === opt ? [] : [opt];
      }
      return next;
    });
  };

  const buildAnswers = (): string[][] =>
    questions.map((_, i) => {
      const vals = [...selected[i]];
      const ft = (freeText[i] ?? "").trim();
      if (ft) vals.push(ft);
      return vals;
    });

  const answered =
    selected[idx].length > 0 || (freeText[idx] ?? "").trim() !== "";
  const freeTextActive = (freeText[idx] ?? "").trim() !== "";
  const freeTextOptionClassName =
    "flex items-center gap-2 rounded-lg border border-border-subtle bg-surface px-3 py-2 text-sm " +
    (freeTextActive
      ? "text-foreground ring-1 ring-primary"
      : "text-foreground-secondary focus-within:ring-1 focus-within:ring-ring");

  return (
    <div className="m-3 flex shrink-0 flex-col gap-2 rounded-lg border border-border-subtle bg-background shadow-sm">
      <div className="flex items-center bg-card gap-2 py-3 px-4 text-xs text-foreground-muted">
        {q.header && (
          <span className="rounded bg-card px-2 py-0.5 font-medium text-foreground-secondary">
            {q.header}
          </span>
        )}
        <span>{q.multiSelect ? "多选" : "单选"}</span>
        {agentLabel && <span className="truncate text-foreground-secondary">· {agentLabel} 提问</span>}
        <span className="ml-auto tabular-nums">
          {idx + 1}/{total}
        </span>
        {onCancel && (
          <Tooltip content="取消回答并停止">
            <button
              type="button"
              aria-label="取消回答并停止"
              onClick={onCancel}
              className="grid h-6 w-6 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
            >
              <X className="h-3.5 w-3.5" aria-hidden="true" />
            </button>
          </Tooltip>
        )}
      </div>
      <div className="flex flex-col gap-4 px-4 py-3">
        <div >
      <MarkdownText
        value={q.question}
        className="max-w-full text-sm font-medium text-foreground [overflow-wrap:anywhere]"
          />
          </div>

      <div className="flex max-h-60 flex-col gap-2 overflow-auto pb-2 px-2 py-2">
        {visibleOptions.map((opt) => {
          const on = selected[idx].includes(opt);
          return (
            <div
              key={opt}
              onClick={() => !busy && toggle(opt)}
              className={
                "flex cursor-pointer items-center gap-2 rounded-lg border border-border-subtle bg-surface px-3 py-2 text-sm " +
                (on
                  ? "text-foreground ring-1 ring-primary"
                  : "text-foreground-secondary hover:bg-accent")
              }
            >
              <span
                className={
                  (q.multiSelect ? "rounded " : "rounded-full ") +
                  "h-4 w-4 shrink-0 border border-border-strong " +
                  (on
                    ? "border-primary bg-primary"
                    : "border-border-subtle")
                }
              />
              <MarkdownText
                value={opt}
                className="max-w-full [overflow-wrap:anywhere]"
              />
            </div>
          );
        })}
        <div className={freeTextOptionClassName}>
          <span className="h-4 w-4 shrink-0 rounded-full border border-border-subtle" />
          <input
            aria-label="其他自由补充"
            className="min-w-0 flex-1 bg-transparent text-sm text-foreground placeholder:text-foreground-muted focus:outline-none"
            placeholder={visibleOptions.length > 0 ? "其他（自由补充）" : "在此输入你的回答"}
            value={freeText[idx] ?? ""}
            disabled={busy}
            onChange={(e) =>
              setFreeText((prev) => {
                const next = [...prev];
                next[idx] = e.target.value;
                return next;
              })
            }
          />
        </div>
      </div>

      </div>
      
      <div className="flex items-center gap-2 py-3 px-4 bg-card">
        <Button disabled={busy || idx === 0} onClick={() => setIdx((i) => i - 1)}>
          上一题
        </Button>
        <span className="ml-auto" />
        {!isLast && (
          <Button disabled={busy} onClick={() => setIdx((i) => i + 1)}>
            {answered ? "下一题" : "跳过"}
          </Button>
        )}
        {isLast && (
          <Button
            tone="primary"
            disabled={busy}
            onClick={() => onAnswer(buildAnswers())}
          >
            提交
          </Button>
        )}
        
        </div>
    </div>
  );
}
