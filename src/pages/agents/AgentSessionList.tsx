import { MessageSquarePlus, MessagesSquare, Pencil, Trash2 } from "lucide-react";
import { Button } from "../../components/ui/Button";
import type { SessionInfo } from "../../types";

export function AgentSessionList({
  sessions,
  onDelete,
  onNew,
  onOpen,
  onRename,
}: {
  sessions: SessionInfo[];
  onDelete: (id: string) => void;
  onNew: () => void;
  onOpen: (id: string) => void;
  onRename: (id: string, title: string) => void;
}) {
  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-3 flex items-center justify-between">
          <h3 className="text-sm font-semibold text-foreground">会话 {sessions.length}</h3>
          <Button tone="primary" onClick={onNew}><MessageSquarePlus className="h-4 w-4" aria-hidden="true" /> 新建会话</Button>
        </div>
        {sessions.length === 0 ? (
          <p className="rounded-xl border border-dashed border-border py-12 text-center text-xs text-foreground-muted">还没有会话。新建一个，开始和这个智能体协作。</p>
        ) : (
          <ul className="flex flex-col gap-1.5">
            {sessions.map((session) => (
              <li key={session.id} className="group flex items-center gap-2 rounded-lg border border-border-subtle bg-surface px-3 py-2.5 transition hover:border-border">
                <MessagesSquare className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />
                <button type="button" onClick={() => onOpen(session.id)} className="min-w-0 flex-1 truncate text-left text-[13px] font-medium text-foreground">{session.title}</button>
                <button type="button" title="重命名" onClick={() => { const value = window.prompt("会话名称", session.title); if (value) onRename(session.id, value); }} className="rounded px-1 py-1 text-foreground-muted opacity-0 transition hover:text-foreground group-hover:opacity-100"><Pencil className="h-3.5 w-3.5" aria-hidden="true" /></button>
                <button type="button" title="删除" onClick={() => { if (window.confirm(`删除会话「${session.title}」？`)) onDelete(session.id); }} className="rounded px-1 py-1 text-foreground-muted opacity-0 transition hover:text-destructive group-hover:opacity-100"><Trash2 className="h-3.5 w-3.5" aria-hidden="true" /></button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
