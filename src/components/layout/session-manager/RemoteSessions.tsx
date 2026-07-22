import { ChevronRight, Loader2, Radio, Settings2 } from "lucide-react";
import { Tooltip } from "../../ui";
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
  onOpenRemoteConfig,
  onOpenSession,
  onToggleChannel,
  remoteBindings,
  remoteExpanded,
  sessions,
}: {
  currentSessionId: string | null;
  /** 打开 IM 渠道配置页（悬浮齿轮）。渠道配置已从侧栏移入本区块，见下方常驻说明。 */
  onOpenRemoteConfig: () => void;
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

  // 注意：**不能**在无远程会话时 return null。渠道配置入口（齿轮）挂在本区块表头上，
  // 若无会话就整块不渲染，用户将永远打不开配置——而没配置就不可能有远程会话（死锁）。
  // 故表头常驻；仅「渠道列表」部分在无会话时显示空态。

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
        <span className="flex max-w-0 shrink-0 items-center gap-1 overflow-hidden opacity-0 transition-all duration-150 group-hover:ml-1 group-hover:max-w-[44px] group-hover:opacity-100 group-focus-within:ml-1 group-focus-within:max-w-[44px] group-focus-within:opacity-100">
          <Tooltip content="渠道配置">
            <button
              type="button"
              aria-label="渠道配置"
              onClick={(event) => {
                event.stopPropagation();
                onOpenRemoteConfig();
              }}
              className="grid h-5 w-5 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
            >
              <Settings2 className="h-3 w-3" aria-hidden="true" />
            </button>
          </Tooltip>
        </span>
      </div>
      {sectionExpanded && remoteSections.length === 0 && (
        <button
          type="button"
          onClick={onOpenRemoteConfig}
          className="mx-2 rounded-md px-2 py-1.5 text-left text-xs text-foreground-muted transition hover:bg-accent hover:text-foreground"
        >
          还没有远程会话，点此配置 IM 渠道
        </button>
      )}
      {sectionExpanded && remoteSections.length > 0 && (
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
