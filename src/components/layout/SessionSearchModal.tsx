import { useEffect, useMemo, useState, type KeyboardEvent } from "react";
import { Loader2, Search } from "lucide-react";
import { listSessions } from "../../api";
import type { SessionInfo } from "../../types";
import { Modal, Tooltip } from "../ui";
import { useSession } from "../session/SessionProvider";

function sessionSearchText(session: SessionInfo): string {
  return [
    session.title,
    session.draftContent,
    session.updatedAt,
    session.createdAt,
  ]
    .filter((value): value is string => Boolean(value))
    .join(" ")
    .toLowerCase();
}

function isTopLevelSession(session: SessionInfo): boolean {
  return !session.parentSessionId;
}

function sessionLabel(session: SessionInfo): string {
  if (session.isDraft) {
    const firstLine = (session.draftContent ?? "")
      .replace(/⟦@[^⟧]+⟧/g, "")
      .replace(/⟦技能：([^⟧]+)⟧/g, "$1")
      .split("\n")
      .map((line) => line.trim())
      .find((line) => line.length > 0);
    return firstLine || "未命名草稿";
  }
  return session.title || "未命名会话";
}

function sessionKindLabel(session: SessionInfo): string {
  if (session.isDraft) return "草稿";
  if (session.origin === "remote") return "远程会话";
  if (session.origin === "scheduled") return "定时任务";
  return "会话";
}

function formatSessionAge(value: string): string {
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) return value;
  const diffMs = Date.now() - timestamp;
  const diffDays = Math.floor(diffMs / 86_400_000);
  if (diffDays <= 0) return "今天";
  if (diffDays === 1) return "昨天";
  if (diffDays < 30) return `${diffDays} 天前`;
  const diffMonths = Math.floor(diffDays / 30);
  if (diffMonths < 12) return `${diffMonths} 个月前`;
  return `${Math.floor(diffMonths / 12)} 年前`;
}

export function SessionSearchModal({
  onClose,
  open,
}: {
  onClose: () => void;
  open: boolean;
}) {
  const { currentSessionId, openDraft, openSession } = useSession();
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setQuery("");
    setActiveIndex(0);
    setLoading(true);
    setError(null);
    listSessions()
      .then((list) => {
        if (cancelled) return;
        setSessions(
          list
            .filter(isTopLevelSession)
            .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)),
        );
      })
      .catch((err) => {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  const results = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    const filtered = normalized
      ? sessions.filter((session) => sessionSearchText(session).includes(normalized))
      : sessions;
    return filtered.slice(0, 40);
  }, [query, sessions]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query]);

  useEffect(() => {
    if (activeIndex >= results.length) {
      setActiveIndex(Math.max(0, results.length - 1));
    }
  }, [activeIndex, results.length]);

  function openResult(session: SessionInfo) {
    onClose();
    if (session.isDraft) {
      openDraft(session.id);
    } else {
      openSession(session.id);
    }
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.metaKey && /^[1-9]$/.test(event.key)) {
      event.preventDefault();
      const session = results[Number(event.key) - 1];
      if (session) openResult(session);
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveIndex((index) => Math.min(index + 1, Math.max(0, results.length - 1)));
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((index) => Math.max(0, index - 1));
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const session = results[activeIndex];
      if (session) openResult(session);
    }
  }

  return (
    <Modal
      className="max-w-[560px] overflow-hidden"
      open={open}
      onClose={onClose}
      padding="none"
      title="搜索会话"
    >
      <div className="flex flex-col">
        <div className="border-b border-border-subtle px-4 py-3">
          <label className="flex h-11 items-center gap-3 text-sm">
            <Search className="h-5 w-5 shrink-0 text-foreground-muted" aria-hidden="true" />
            <input
              autoFocus
              aria-label="搜索会话"
              className="h-full min-w-0 flex-1 bg-transparent text-[16px] text-foreground outline-none placeholder:text-foreground-muted"
              placeholder="搜索会话内容..."
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={handleKeyDown}
            />
          </label>
        </div>

        <div className="flex h-10 items-center justify-between border-b border-border-subtle px-4 text-sm">
          <span className="font-semibold text-foreground-secondary">所有任务</span>
          <span className="text-foreground-muted">共 {results.length} 个</span>
        </div>

        <div className="max-h-[420px] overflow-y-auto px-2 py-1.5" role="listbox">
          {loading ? (
            <div className="flex items-center justify-center gap-2 px-3 py-8 text-sm text-foreground-muted">
              <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
              正在加载会话
            </div>
          ) : error ? (
            <div className="px-3 py-8 text-center text-sm text-destructive">
              {error}
            </div>
          ) : results.length === 0 ? (
            <div className="px-3 py-8 text-center text-sm text-foreground-muted">
              没有匹配的会话
            </div>
          ) : (
            results.map((session, index) => {
              const active = index === activeIndex;
              const selected = session.id === currentSessionId;
              return (
                <button
                  key={session.id}
                  type="button"
                  role="option"
                  aria-selected={active}
                  className={`flex h-11 w-full items-center gap-3 rounded-lg pl-5 pr-3 text-left transition ${
                    active ? "bg-accent text-accent-foreground" : "text-foreground-secondary"
                  }`}
                  onMouseEnter={() => setActiveIndex(index)}
                  onClick={() => openResult(session)}
                >
                  <Tooltip content={sessionLabel(session)}>
                  <span className="min-w-0 flex-1 truncate text-[13px] font-medium">
                    {sessionLabel(session)}
                    </span>
                    </Tooltip>
                  {index < 9 ? (
                    <span className="flex shrink-0 items-center gap-1 text-xs text-foreground-secondary">
                      <kbd className="grid h-6 min-w-7 place-items-center rounded-md border border-border-subtle bg-muted px-1.5 font-medium">
                        ⌘
                      </kbd>
                      <kbd className="grid h-6 min-w-6 place-items-center rounded-md border border-border-subtle bg-muted px-1.5 font-medium">
                        {index + 1}
                      </kbd>
                    </span>
                  ) : (
                    <span className="shrink-0 text-xs text-foreground-muted">
                      {sessionKindLabel(session)} · {formatSessionAge(session.updatedAt)}
                    </span>
                  )}
                </button>
              );
            })
          )}
        </div>
      </div>
    </Modal>
  );
}
