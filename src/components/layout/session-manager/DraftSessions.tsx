import { Trash2 } from "lucide-react";
import { Tooltip } from "../../../components/ui";
import type { SessionInfo } from "../../../types";
import { byUpdatedDesc, draftTitle } from "./sessionManagerShared";

export function DraftSessions({
  currentSessionId,
  onDeleteDraft,
  onOpenDraft,
  sessions,
}: {
  currentSessionId: string | null;
  onDeleteDraft: (session: SessionInfo) => void;
  onOpenDraft: (sessionId: string) => void;
  sessions: SessionInfo[];
}) {
  const drafts = sessions
    .filter((s) => (!s.origin || s.origin === "user") && s.isDraft)
    .sort(byUpdatedDesc);

  if (drafts.length === 0) return null;

  function draftRow(session: SessionInfo) {
    const title = draftTitle(session);
    const active = currentSessionId === session.id;
    const rowTone = active
      ? "bg-primary font-medium text-[#ffffff]"
      : "text-foreground-secondary hover:bg-accent hover:text-accent-foreground";
    const label = (
      <span className="block min-w-0 flex-1 truncate text-[13px] leading-none">
        {title}
      </span>
    );
    return (
      <div
        key={session.id}
        role="button"
        tabIndex={0}
        onClick={() => onOpenDraft(session.id)}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onOpenDraft(session.id);
          }
        }}
        className={`group flex h-[34px] w-full cursor-pointer items-center gap-1.5 rounded-sm py-1 pr-1 text-left transition ${rowTone}`}
        style={{ paddingLeft: 10 }}
      >
        <Tooltip content={session.draftContent || "未命名草稿"}>{label}</Tooltip>
        <Tooltip content="删除草稿">
          <button
            type="button"
            aria-label="删除草稿"
            onClick={(event) => {
              event.stopPropagation();
              onDeleteDraft(session);
            }}
            className="grid h-6 w-6 shrink-0 place-items-center rounded-md text-foreground-muted opacity-0 transition hover:bg-destructive/10 hover:text-destructive focus:opacity-100 group-hover:opacity-100"
          >
            <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
        </Tooltip>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-0.5">
      {drafts.map(draftRow)}
    </div>
  );
}
