import { ChevronRight, Loader2, Radio } from "lucide-react";
import type { RemoteBinding } from "../../../api/remote";
import type { SessionInfo } from "../../../types";
import { GroupRow, ItemRow } from "./SessionRows";
import {
  byUpdatedDesc,
  remoteChannelIcon,
  remoteChannels,
  type RemoteSessionItem,
  shortPeerId,
} from "./sessionManagerShared";

export function RemoteSessions({
  currentSessionId,
  onOpenSession,
  onToggleChannel,
  remoteBindings,
  remoteExpanded,
  sessions,
}: {
  currentSessionId: string | null;
  onOpenSession: (sessionId: string) => void;
  onToggleChannel: (channelId: string) => void;
  remoteBindings: RemoteBinding[];
  remoteExpanded: Set<string>;
  sessions: SessionInfo[];
}) {
  const sessionsById = new Map(sessions.map((session) => [session.id, session]));
  const remoteSections = remoteChannels
    .map((channel) => {
      const items = remoteBindings
        .filter((binding) => binding.channel === channel.id)
        .map((binding): RemoteSessionItem | null => {
          const session = sessionsById.get(binding.sessionId);
          if (!session || session.origin !== "remote") return null;
          return { binding, session };
        })
        .filter((item): item is RemoteSessionItem => item !== null)
        .sort((a, b) => byUpdatedDesc(a.session, b.session));
      return { channel, items };
    })
    .filter((section) => section.items.length > 0);

  if (remoteSections.length === 0) return null;

  function remoteSessionRow(item: RemoteSessionItem) {
    const channel = remoteChannels.find((candidate) => candidate.id === item.binding.channel);
    const channelLabel = channel?.label ?? item.binding.channel;
    const accountName =
      item.binding.accountName || `${channelLabel} ${shortPeerId(item.binding.peerId)}`;
    const sessionTitle = item.session.title || "未命名会话";
    return (
      <ItemRow
        key={`${item.binding.channel}:${item.binding.peerId}:${item.session.id}`}
        active={item.session.id === currentSessionId}
        label={(
        <>
          <span>{accountName}</span>
          <span className="mx-1">·</span>
          <span>{sessionTitle}</span>
        </>
        )}
        onClick={() => onOpenSession(item.session.id)}
        tooltip={`${channelLabel} / ${accountName} / ${sessionTitle}`}
        trailing={item.session.isRunning ? (
        <Loader2
          className="h-3.5 w-3.5 shrink-0 animate-spin text-foreground-muted"
          aria-hidden="true"
        />
      ) : undefined}
      />
    );
  }

  const sectionExpanded = remoteExpanded.has("__remote_section__");

  return (
    <div className="flex flex-col gap-0.5">
      <div
        role="button"
        tabIndex={0}
        onClick={() => onToggleChannel("__remote_section__")}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onToggleChannel("__remote_section__");
          }
        }}
        className="group flex h-7 cursor-pointer items-center gap-1.5 px-2 text-foreground-muted"
      >
        <span className="min-w-0 truncate text-[13px] uppercase leading-none">
          远程
        </span>
        {/* <span className="shrink-0 text-xs font-normal">·</span>
        <span className="shrink-0 text-xs font-normal">
          {remoteSections.reduce((total, section) => total + section.items.length, 0)}
        </span> */}
        <ChevronRight
          className={`h-3.5 w-3.5 shrink-0 opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100 ${sectionExpanded ? "rotate-90" : ""}`}
          aria-hidden="true"
        />
        <span className="min-w-0 flex-1" />
      </div>
      {sectionExpanded && (
        <div className="flex flex-col gap-0.5">
          {remoteSections.map(({ channel, items }) => (
            <GroupRow
              key={channel.id}
              badge={items.length}
              expanded={remoteExpanded.has(channel.id)}
              icon={remoteChannelIcon(channel.id)}
              label={channel.label}
              onToggle={() => onToggleChannel(channel.id)}
            >
              {items.map(remoteSessionRow)}
            </GroupRow>
          ))}
        </div>
      )}
    </div>
  );
}
