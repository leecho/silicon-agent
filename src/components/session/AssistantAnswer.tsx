import { useRef } from "react";
import { ClipboardList, Copy } from "lucide-react";
import { MarkdownText } from "../ui/MarkdownText";
import { Tooltip } from "../ui/Tooltip";
import { useNotifications } from "../ui/NotificationProvider";

export function AssistantAnswer({ markdown }: { markdown: string }) {
  const notifications = useNotifications();
  const answerRef = useRef<HTMLDivElement | null>(null);

  async function copyText(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      notifications.success({
        title: "已复制",
        message: label,
      });
    } catch (err) {
      notifications.error({
        title: "复制失败",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  const copyPlainText = () => {
    const text = answerRef.current?.innerText.trim() || markdown.trim();
    void copyText(text, "");
  };

  const copyMarkdown = async () => {
    try {
      await navigator.clipboard.writeText(markdown);
      notifications.success({
        title: "已复制",
        message: "",
      });
    } catch (err) {
      notifications.error({
        title: "复制失败",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  };

  return (
    <div className="min-w-0 max-w-full pt-3">
      <div ref={answerRef} data-quote-source="assistant">
        <MarkdownText
          value={markdown}
          className="max-w-full [overflow-wrap:anywhere]"
        />
      </div>
      <div className="mt-1.5 flex justify-start gap-1.5">
        <Tooltip content="复制">
          <button
            type="button"
            className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-xs font-medium text-foreground-muted transition hover:bg-accent hover:text-foreground"
            onClick={copyPlainText}
          >
            <Copy className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
        </Tooltip>
        <Tooltip content="复制成Markdown">
          <button
            type="button"
            className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-xs font-medium text-foreground-muted transition hover:bg-accent hover:text-foreground"
            onClick={() => void copyMarkdown()}
          >
            <ClipboardList className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
        </Tooltip>
      </div>
    </div>
  );
}
