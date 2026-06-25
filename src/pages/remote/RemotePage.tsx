import type { InputHTMLAttributes, ReactNode } from "react";
import { useEffect, useMemo, useState } from "react";
import {
  Bot,
  CheckCircle2,
  CircleSlash,
  Info,
  KeyRound,
  Loader2,
  MessageSquare,
  Pause,
  Play,
  Plug,
  QrCode,
  RotateCw,
  Send,
  ShieldCheck,
  Smartphone,
  Trash2,
  Unplug,
} from "lucide-react";
import QRCode from "qrcode";
import {
  beginWechatPairing,
  connectDingtalk,
  connectFeishu,
  connectTelegram,
  disconnectRemoteChannel,
  listRemoteAllowlist,
  listRemoteBindings,
  listRemoteChannels,
  onPairingEvent,
  pauseRemoteChannel,
  removeRemotePeer,
  resumeRemoteChannel,
  type AllowedPeer,
  type PairingPhase,
  type RemoteBinding,
  type RemoteChannelConfig,
} from "../../api/remote";
import { listSessions } from "../../api";
import { Badge, Button, Message, Modal, ModalHeader, Tooltip, useMessages } from "../../components/ui";
import type { SessionInfo } from "../../types";

type ChannelId = "wechat" | "telegram" | "dingtalk" | "feishu";
type RemoteChannelStatus = RemoteChannelConfig["status"];

/** 配对面板状态。 */
type Pairing =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "qr"; image: string | null; code: string; phase: PairingPhase }
  | { kind: "done" }
  | { kind: "error"; message: string };

type ConnectionState =
  | { kind: "idle" }
  | { kind: "connecting" }
  | { kind: "done" }
  | { kind: "error"; message: string };

type ChannelMeta = {
  id: ChannelId;
  title: string;
  description: string;
  icon: ReactNode;
};

const CHANNELS: ChannelMeta[] = [
  {
    id: "wechat",
    title: "微信（ClawBot）",
    description: "扫码连接后，可以直接在微信里收发消息。",
    icon: <Smartphone className="h-5 w-5" aria-hidden="true" />,
  },
  {
    id: "telegram",
    title: "Telegram",
    description: "连接后，可以直接在 Telegram 里收发消息。",
    icon: <Send className="h-5 w-5" aria-hidden="true" />,
  },
  {
    id: "dingtalk",
    title: "钉钉",
    description: "连接后，可以直接在钉钉里收发消息。",
    icon: <MessageSquare className="h-5 w-5" aria-hidden="true" />,
  },
  {
    id: "feishu",
    title: "飞书 / Lark",
    description: "连接后，可以直接在飞书里收发消息。",
    icon: <Bot className="h-5 w-5" aria-hidden="true" />,
  },
];

const PHASE_LABEL: Record<PairingPhase, string> = {
  qr: "请用微信扫描二维码",
  scanned: "已扫描，请在手机上确认…",
  confirmed: "绑定成功",
  expired: "二维码已过期，请重试",
  error: "配对失败",
};

const INPUT_CLASS =
  "h-10 min-w-0 rounded-lg border border-border bg-background px-3 text-sm text-foreground outline-none transition placeholder:text-foreground-muted focus:border-ring";

function TextInput(props: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...props} className={`${INPUT_CLASS} ${props.className ?? ""}`} />;
}

function StateMessage({ state, successText }: { state: ConnectionState; successText: string }) {
  if (state.kind === "done") {
    return (
      <Message tone="success" className="text-xs leading-5">
        {successText}
      </Message>
    );
  }
  if (state.kind === "error") {
    return (
      <Message tone="danger" className="text-xs leading-5">
        {state.message}
      </Message>
    );
  }
  return null;
}

function statusLabel(status: RemoteChannelStatus) {
  if (status === "connected") return "已连接";
  if (status === "connecting") return "连接中";
  if (status === "paused") return "已暂停";
  if (status === "error") return "连接异常";
  return "未连接";
}

function statusTone(status: RemoteChannelStatus): "neutral" | "success" | "warning" | "danger" | "running" {
  if (status === "connected") return "success";
  if (status === "connecting") return "running";
  if (status === "paused") return "warning";
  if (status === "error") return "danger";
  return "neutral";
}

function IconActionButton({
  busy,
  children,
  disabled,
  label,
  onClick,
  tone = "secondary",
}: {
  busy?: boolean;
  children: ReactNode;
  disabled?: boolean;
  label: string;
  onClick?: () => void;
  tone?: "primary" | "secondary" | "danger";
}) {
  const toneClass =
    tone === "primary"
      ? "border-primary/20 bg-primary/10 text-primary hover:bg-primary/15"
      : tone === "danger"
        ? "border-destructive/20 bg-destructive/10 text-destructive hover:bg-destructive/15"
        : "border-border bg-background text-foreground-muted hover:bg-accent hover:text-foreground";

  const button = (
    <button
      type="button"
      aria-label={label}
      className={`grid h-8 w-8 shrink-0 place-items-center rounded-lg border transition disabled:cursor-not-allowed disabled:opacity-50 ${toneClass}`}
      disabled={disabled || busy}
      onClick={onClick}
      title={label}
    >
      {busy ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : children}
    </button>
  );

  return <Tooltip content={label}>{button}</Tooltip>;
}

function RemoteRow({
  connectBusy,
  isLast,
  lifecycleBusy,
  meta,
  onConnect,
  onDetails,
  onDisconnect,
  onPause,
  onResume,
  status,
}: {
  connectBusy?: boolean;
  isLast: boolean;
  lifecycleBusy?: "pause" | "resume" | "disconnect";
  meta: ChannelMeta;
  onConnect: () => void;
  onDetails: () => void;
  onDisconnect: () => void;
  onPause: () => void;
  onResume: () => void;
  status: RemoteChannelStatus;
}) {
  const isConnecting = status === "connecting" || connectBusy;

  function renderLifecycleActions() {
    if (isConnecting) {
      return (
        <IconActionButton busy label="连接中" tone="primary">
          <Loader2 className="h-4 w-4" aria-hidden="true" />
        </IconActionButton>
      );
    }

    if (status === "disconnected") {
      return (
        <IconActionButton label="连接" onClick={onConnect} tone="primary">
          <Plug className="h-4 w-4" aria-hidden="true" />
        </IconActionButton>
      );
    }

    if (status === "connected") {
      return (
        <>
          <IconActionButton
            busy={lifecycleBusy === "pause"}
            label={lifecycleBusy === "pause" ? "暂停中" : "暂停"}
            onClick={onPause}
          >
            <Pause className="h-4 w-4" aria-hidden="true" />
          </IconActionButton>
          <IconActionButton
            busy={lifecycleBusy === "disconnect"}
            label={lifecycleBusy === "disconnect" ? "解除中" : "解除连接"}
            onClick={onDisconnect}
            tone="danger"
          >
            <Unplug className="h-4 w-4" aria-hidden="true" />
          </IconActionButton>
        </>
      );
    }

    if (status === "paused") {
      return (
        <>
          <IconActionButton
            busy={lifecycleBusy === "resume"}
            label={lifecycleBusy === "resume" ? "恢复中" : "恢复连接"}
            onClick={onResume}
            tone="primary"
          >
            <Play className="h-4 w-4" aria-hidden="true" />
          </IconActionButton>
          <IconActionButton
            busy={lifecycleBusy === "disconnect"}
            label={lifecycleBusy === "disconnect" ? "解除中" : "解除连接"}
            onClick={onDisconnect}
            tone="danger"
          >
            <Unplug className="h-4 w-4" aria-hidden="true" />
          </IconActionButton>
        </>
      );
    }

    return (
      <>
        <IconActionButton label="重新连接" onClick={onConnect} tone="primary">
          <RotateCw className="h-4 w-4" aria-hidden="true" />
        </IconActionButton>
        <IconActionButton
          busy={lifecycleBusy === "disconnect"}
          label={lifecycleBusy === "disconnect" ? "解除中" : "解除连接"}
          onClick={onDisconnect}
          tone="danger"
        >
          <Unplug className="h-4 w-4" aria-hidden="true" />
        </IconActionButton>
      </>
    );
  }

  return (
    <li
      className={`group flex flex-wrap items-center gap-3.5 px-4 py-4 transition-colors hover:bg-accent ${
        isLast ? "" : "border-b border-border-subtle"
      }`}
    >
      <div
        className={`grid h-10 w-10 shrink-0 place-items-center rounded-lg border border-border bg-background shadow-sm transition-colors ${
          status === "connected" ? "text-primary" : "text-foreground-muted"
        }`}
      >
        {meta.icon}
      </div>

      <button type="button" className="min-w-0 flex-1 text-left" onClick={onDetails}>
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <h3 className="truncate font-semibold text-foreground">{meta.title}</h3>
          <Badge className="px-1.5 py-0 text-[10px] leading-4" tone={statusTone(status)}>
            {statusLabel(status)}
          </Badge>
        </div>
        <p className="mt-0.5 line-clamp-1 text-xs text-foreground-secondary">{meta.description}</p>
      </button>

      <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
        {renderLifecycleActions()}
        <IconActionButton label="详情" onClick={onDetails}>
          <Info className="h-4 w-4" aria-hidden="true" />
        </IconActionButton>
      </div>
    </li>
  );
}

function DialogTitle({ meta, title }: { meta: ChannelMeta; title: string }) {
  return (
    <div className="flex min-w-0 items-center gap-3">
      <span className="grid h-10 w-10 shrink-0 place-items-center rounded-lg border border-border bg-background text-foreground-secondary shadow-sm">
        {meta.icon}
      </span>
      <div className="min-w-0">
        <h2 className="truncate text-sm font-semibold text-foreground">{title}</h2>
        <p className="mt-1 text-xs text-foreground-muted">{meta.description}</p>
      </div>
    </div>
  );
}

function DialogSection({
  children,
  title,
}: {
  children: ReactNode;
  title: string;
}) {
  return (
    <section className="grid gap-3">
      <h3 className="text-sm font-semibold text-foreground">{title}</h3>
      {children}
    </section>
  );
}

function ChannelConnectModal({
  dtKey,
  dtSecret,
  dtState,
  fsId,
  fsSecret,
  fsState,
  meta,
  mode,
  onClose,
  onConnectDingtalk,
  onConnectFeishu,
  onConnectTelegram,
  onPairWechat,
  onUpdateDtKey,
  onUpdateDtSecret,
  onUpdateFsId,
  onUpdateFsSecret,
  onUpdateTgToken,
  open,
  pairing,
  tgState,
  tgToken,
}: {
  dtKey: string;
  dtSecret: string;
  dtState: ConnectionState;
  fsId: string;
  fsSecret: string;
  fsState: ConnectionState;
  meta: ChannelMeta | null;
  mode: "connect" | "reconnect";
  onClose: () => void;
  onConnectDingtalk: () => void;
  onConnectFeishu: () => void;
  onConnectTelegram: () => void;
  onPairWechat: () => void;
  onUpdateDtKey: (value: string) => void;
  onUpdateDtSecret: (value: string) => void;
  onUpdateFsId: (value: string) => void;
  onUpdateFsSecret: (value: string) => void;
  onUpdateTgToken: (value: string) => void;
  open: boolean;
  pairing: Pairing;
  tgState: ConnectionState;
  tgToken: string;
}) {
  if (!meta) return null;

  const reconnecting = mode === "reconnect";

  return (
    <Modal
      className="max-w-[420px] overflow-hidden"
      open={open}
      onClose={onClose}
      title={`${meta.title} 连接`}
      padding="none"
    >
      <div className="border-b border-border bg-surface px-5 py-3">
        <ModalHeader onClose={onClose}>
          <DialogTitle meta={meta} title={reconnecting ? `重新连接 ${meta.title}` : `连接 ${meta.title}`} />
        </ModalHeader>
      </div>

      <div className="max-h-[70vh] overflow-auto p-5">
        {meta.id === "wechat" && (
          <div className="grid gap-4">
            <WechatConnectPanel mode={mode} pairing={pairing} />
            <div className="flex justify-end">
              <Button tone="primary" onClick={onPairWechat} disabled={pairing.kind === "loading"}>
                {pairing.kind === "loading" && <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />}
                <QrCode className="h-4 w-4" aria-hidden="true" />
                {reconnecting ? "重新获取二维码" : "获取二维码"}
              </Button>
            </div>
          </div>
        )}

        {meta.id === "telegram" && (
          <div className="grid gap-4">
            <TextInput
              type="password"
              value={tgToken}
              onChange={(e) => onUpdateTgToken(e.target.value)}
              placeholder="请输入 Telegram 连接口令"
            />
            <StateMessage state={tgState} successText="已连接。现在可以在 Telegram 里发消息试试。" />
            <div className="flex justify-end">
              <Button
                tone="primary"
                onClick={onConnectTelegram}
                disabled={tgState.kind === "connecting" || !tgToken.trim()}
              >
                {tgState.kind === "connecting" && <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />}
                {tgState.kind === "connecting" ? "连接中…" : reconnecting ? "重新连接" : "连接"}
              </Button>
            </div>
          </div>
        )}

        {meta.id === "dingtalk" && (
          <div className="grid gap-4">
            <div className="grid gap-3">
              <TextInput value={dtKey} onChange={(e) => onUpdateDtKey(e.target.value)} placeholder="请输入钉钉应用编号" />
              <TextInput
                type="password"
                value={dtSecret}
                onChange={(e) => onUpdateDtSecret(e.target.value)}
                placeholder="请输入钉钉应用密钥"
              />
            </div>
            <StateMessage state={dtState} successText="已连接。现在可以在钉钉里发消息试试。" />
            <div className="flex justify-end">
              <Button
                tone="primary"
                onClick={onConnectDingtalk}
                disabled={dtState.kind === "connecting" || !dtKey.trim() || !dtSecret.trim()}
              >
                {dtState.kind === "connecting" && <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />}
                {dtState.kind === "connecting" ? "连接中…" : reconnecting ? "重新连接" : "连接"}
              </Button>
            </div>
          </div>
        )}

        {meta.id === "feishu" && (
          <div className="grid gap-4">
            <div className="grid gap-3">
              <TextInput value={fsId} onChange={(e) => onUpdateFsId(e.target.value)} placeholder="请输入飞书应用编号" />
              <TextInput
                type="password"
                value={fsSecret}
                onChange={(e) => onUpdateFsSecret(e.target.value)}
                placeholder="请输入飞书应用密钥"
              />
            </div>
            <StateMessage state={fsState} successText="已连接。现在可以在飞书里发消息试试。" />
            <div className="flex justify-end">
              <Button
                tone="primary"
                onClick={onConnectFeishu}
                disabled={fsState.kind === "connecting" || !fsId.trim() || !fsSecret.trim()}
              >
                {fsState.kind === "connecting" && <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />}
                {fsState.kind === "connecting" ? "连接中…" : reconnecting ? "重新连接" : "连接"}
              </Button>
            </div>
          </div>
        )}
      </div>
    </Modal>
  );
}

function EmptyState({ children, icon }: { children: ReactNode; icon: ReactNode }) {
  return (
    <div className="grid place-items-center rounded-lg border border-dashed border-border-subtle bg-background px-4 py-7 text-center">
      <div className="grid h-10 w-10 place-items-center rounded-lg bg-card text-foreground-muted">
        {icon}
      </div>
      <p className="mt-3 text-xs leading-5 text-foreground-muted">{children}</p>
    </div>
  );
}

function ChannelDetailModal({
  bindings,
  meta,
  onClose,
  onOpenSession,
  onRemovePeer,
  open,
  peers,
  sessions,
}: {
  bindings: RemoteBinding[];
  meta: ChannelMeta | null;
  onClose: () => void;
  onOpenSession: (sessionId: string) => void;
  onRemovePeer: (peer: AllowedPeer) => void;
  open: boolean;
  peers: AllowedPeer[];
  sessions: SessionInfo[];
}) {
  if (!meta) return null;
  const sessionsById = new Map(sessions.map((session) => [session.id, session]));

  return (
    <Modal
      className="max-w-[440px] overflow-hidden"
      open={open}
      onClose={onClose}
      title={`${meta.title} 详情`}
      padding="none"
    >
      <div className="border-b border-border bg-surface px-5 py-3">
        <ModalHeader onClose={onClose}>
          <DialogTitle meta={meta} title={meta.title} />
        </ModalHeader>
      </div>

      <div className="grid max-h-[70vh] content-start gap-5 overflow-auto p-5">
        <DialogSection title="可使用的人">
          {peers.length === 0 ? (
            <EmptyState icon={<CircleSlash className="h-5 w-5" aria-hidden="true" />}>
              暂无联系人。连接后，对方发来消息就会显示在这里。
            </EmptyState>
          ) : (
            <ul className="flex max-h-72 flex-col gap-2 overflow-auto pr-1">
              {peers.map((peer) => (
                <li
                  key={`${peer.channel}/${peer.peerId}`}
                  className="group flex min-w-0 items-center gap-3 rounded-lg border border-border-subtle bg-surface px-3 py-2.5"
                >
                  <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-accent text-foreground-muted">
                    <ShieldCheck className="h-4 w-4" aria-hidden="true" />
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium text-foreground">
                      {peer.label ?? "联系人"}
                    </div>
                    <div className="mt-1 truncate text-[11px] text-foreground-muted">
                      已授权使用
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => onRemovePeer(peer)}
                    className="grid h-8 w-8 shrink-0 place-items-center rounded-lg text-foreground-muted opacity-80 transition hover:bg-destructive/10 hover:text-destructive focus:bg-destructive/10 focus:text-destructive focus:outline-none group-hover:opacity-100"
                    aria-label="移除"
                  >
                    <Trash2 className="h-4 w-4" aria-hidden="true" />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </DialogSection>

        <DialogSection title="关联对话">
          {bindings.length === 0 ? (
            <EmptyState icon={<KeyRound className="h-5 w-5" aria-hidden="true" />}>
              暂无关联对话。开始聊天后会在这里显示。
            </EmptyState>
          ) : (
            <ul className="flex max-h-72 flex-col gap-2 overflow-auto pr-1">
              {bindings.map((binding) => {
                const session = sessionsById.get(binding.sessionId);
                const title = session?.title?.trim() || "未命名会话";
                return (
                  <li
                    key={`${binding.channel}/${binding.peerId}`}
                    className="grid min-w-0 grid-cols-[auto_minmax(0,1fr)] gap-3 rounded-lg border border-border-subtle bg-surface px-3 py-2.5 transition hover:border-border hover:bg-accent"
                  >
                    <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-accent text-foreground-muted">
                      <CheckCircle2 className="h-4 w-4" aria-hidden="true" />
                    </span>
                    <div className="min-w-0">
                      <div className="flex min-w-0 items-start gap-2">
                        <button
                          type="button"
                          className="min-w-0 flex-1 text-left"
                          onClick={() => {
                            onClose();
                            onOpenSession(binding.sessionId);
                          }}
                        >
                          <span className="block truncate text-sm font-medium text-foreground">{title}</span>
                          <span className="mt-1 block truncate text-[11px] text-foreground-muted">
                            点击进入这个会话
                          </span>
                        </button>
                        {binding.pendingKind && (
                          <Badge tone="warning" className="shrink-0">
                            等待确认
                          </Badge>
                        )}
                      </div>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </DialogSection>
      </div>
    </Modal>
  );
}

/**
 * 聊天渠道设置：让用户在常用聊天工具里直接对话。
 */
export function RemotePage({ onOpenSession }: { onOpenSession: (sessionId: string) => void }) {
  const messages = useMessages();
  const [channels, setChannels] = useState<RemoteChannelConfig[]>([]);
  const [allowlist, setAllowlist] = useState<AllowedPeer[]>([]);
  const [bindings, setBindings] = useState<RemoteBinding[]>([]);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [connectChannel, setConnectChannel] = useState<ChannelId | null>(null);
  const [detailChannel, setDetailChannel] = useState<ChannelId | null>(null);
  const [lifecycleAction, setLifecycleAction] = useState<{
    channel: ChannelId;
    kind: "pause" | "resume" | "disconnect";
  } | null>(null);
  const [pairing, setPairing] = useState<Pairing>({ kind: "idle" });
  const [tgToken, setTgToken] = useState("");
  const [tgState, setTgState] = useState<ConnectionState>({ kind: "idle" });

  async function connectTg() {
    setTgState({ kind: "connecting" });
    try {
      await connectTelegram(tgToken.trim());
      setTgToken("");
      setTgState({ kind: "done" });
      reload().catch(() => {});
    } catch (err) {
      setTgState({ kind: "error", message: String(err) });
    }
  }

  const [dtKey, setDtKey] = useState("");
  const [dtSecret, setDtSecret] = useState("");
  const [dtState, setDtState] = useState<ConnectionState>({ kind: "idle" });

  async function connectDt() {
    setDtState({ kind: "connecting" });
    try {
      await connectDingtalk(dtKey.trim(), dtSecret.trim());
      setDtKey("");
      setDtSecret("");
      setDtState({ kind: "done" });
      reload().catch(() => {});
    } catch (err) {
      setDtState({ kind: "error", message: String(err) });
    }
  }

  const [fsId, setFsId] = useState("");
  const [fsSecret, setFsSecret] = useState("");
  const [fsState, setFsState] = useState<ConnectionState>({ kind: "idle" });

  async function connectFs() {
    setFsState({ kind: "connecting" });
    try {
      await connectFeishu(fsId.trim(), fsSecret.trim());
      setFsId("");
      setFsSecret("");
      setFsState({ kind: "done" });
      reload().catch(() => {});
    } catch (err) {
      setFsState({ kind: "error", message: String(err) });
    }
  }

  async function reload() {
    const [nextChannels, nextAllowlist, nextBindings, nextSessions] = await Promise.all([
      listRemoteChannels(),
      listRemoteAllowlist(),
      listRemoteBindings(),
      listSessions(),
    ]);
    setChannels(nextChannels);
    setAllowlist(nextAllowlist);
    setBindings(nextBindings);
    setSessions(nextSessions);
  }

  useEffect(() => {
    reload().catch(() => {});
  }, []);

  // 订阅配对状态事件，驱动二维码面板。
  useEffect(() => {
    const un = onPairingEvent((e) => {
      if (e.channel !== "wechat") return;
      if (e.phase === "confirmed") {
        setPairing({ kind: "done" });
        reload().catch(() => {});
      } else if (e.phase === "expired" || e.phase === "error") {
        setPairing({ kind: "error", message: e.message ?? PHASE_LABEL[e.phase] });
      } else {
        setPairing((prev) =>
          prev.kind === "qr" ? { ...prev, phase: e.phase } : prev,
        );
      }
    });
    return () => {
      un.then((f) => f()).catch(() => {});
    };
  }, []);

  async function startPairing() {
    setPairing({ kind: "loading" });
    try {
      const qr = await beginWechatPairing();
      // qrContent 是待编码的深链；本地生成二维码图片（PNG data URL）让用户用微信扫。
      const image = await QRCode.toDataURL(qr.qrContent, { width: 224, margin: 1 });
      setPairing({ kind: "qr", image, code: qr.qrContent, phase: "qr" });
    } catch (err) {
      setPairing({ kind: "error", message: String(err) });
    }
  }

  async function disconnectChannel(channel: ChannelId) {
    const title = channelTitle(channel);
    const ok = await messages.confirm({
      title: "解除聊天连接",
      message: `确定解除「${title}」连接吗？将停止该渠道连接并清除连接配置，之后需要重新连接才能继续使用。`,
      tone: "warning",
      confirmText: "解除连接",
    });
    if (!ok) return;

    setLifecycleAction({ channel, kind: "disconnect" });
    try {
      await disconnectRemoteChannel(channel);
      if (channel === "wechat") setPairing({ kind: "idle" });
      await reload();
    } finally {
      setLifecycleAction(null);
    }
  }

  async function pauseChannel(channel: ChannelId) {
    const title = channelTitle(channel);
    const ok = await messages.confirm({
      title: "暂停聊天连接",
      message: `确定暂停「${title}」吗？暂停后，这个渠道收到的新消息不会继续处理；你可以稍后恢复连接。`,
      tone: "warning",
      confirmText: "暂停",
    });
    if (!ok) return;

    setLifecycleAction({ channel, kind: "pause" });
    try {
      await pauseRemoteChannel(channel);
      await reload();
    } finally {
      setLifecycleAction(null);
    }
  }

  async function resumeChannel(channel: ChannelId) {
    setLifecycleAction({ channel, kind: "resume" });
    try {
      await resumeRemoteChannel(channel);
      await reload();
    } finally {
      setLifecycleAction(null);
    }
  }

  function channelConfig(channel: ChannelId) {
    return channels.find((c) => c.channel === channel);
  }

  function channelStatus(channel: ChannelId): RemoteChannelStatus {
    return channelConfig(channel)?.status ?? "disconnected";
  }

  function channelTitle(channel: ChannelId) {
    return CHANNELS.find((item) => item.id === channel)?.title ?? channel;
  }

  const detailMeta = useMemo(
    () => CHANNELS.find((channel) => channel.id === detailChannel) ?? null,
    [detailChannel],
  );
  const connectMeta = useMemo(
    () => CHANNELS.find((channel) => channel.id === connectChannel) ?? null,
    [connectChannel],
  );
  const detailPeers = detailChannel
    ? allowlist.filter((peer) => peer.channel === detailChannel)
    : [];
  const detailBindings = detailChannel
    ? bindings.filter((binding) => binding.channel === detailChannel)
    : [];

  return (
    <div className="h-full overflow-auto p-6 text-sm">
      <div className="mx-auto max-w-[860px]">
        {/* 头部 */}
        <div className="mb-6 mt-4 flex items-start justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold text-foreground">聊天渠道</h1>
            <p className="mt-1 text-xs text-foreground-muted">
              在常用聊天工具里接收和回复任务消息。
            </p>
          </div>
        </div>
        <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface" aria-label="聊天渠道">
          {CHANNELS.map((meta, index) => {
            const status = channelStatus(meta.id);
            return (
              <RemoteRow
                key={meta.id}
                connectBusy={
                  (meta.id === "wechat" && pairing.kind === "loading") ||
                  (meta.id === "telegram" && tgState.kind === "connecting") ||
                  (meta.id === "dingtalk" && dtState.kind === "connecting") ||
                  (meta.id === "feishu" && fsState.kind === "connecting")
                }
                isLast={index === CHANNELS.length - 1}
                lifecycleBusy={lifecycleAction?.channel === meta.id ? lifecycleAction.kind : undefined}
                meta={meta}
                onConnect={() => setConnectChannel(meta.id)}
                onDetails={() => setDetailChannel(meta.id)}
                onDisconnect={() => disconnectChannel(meta.id)}
                onPause={() => pauseChannel(meta.id)}
                onResume={() => resumeChannel(meta.id)}
                status={status}
              />
            );
          })}
        </ul>

        <ChannelConnectModal
          dtKey={dtKey}
          dtSecret={dtSecret}
          dtState={dtState}
          fsId={fsId}
          fsSecret={fsSecret}
          fsState={fsState}
          meta={connectMeta}
          mode={connectChannel && channelStatus(connectChannel) === "error" ? "reconnect" : "connect"}
          onClose={() => setConnectChannel(null)}
          onConnectDingtalk={connectDt}
          onConnectFeishu={connectFs}
          onConnectTelegram={connectTg}
          onPairWechat={startPairing}
          onUpdateDtKey={setDtKey}
          onUpdateDtSecret={setDtSecret}
          onUpdateFsId={setFsId}
          onUpdateFsSecret={setFsSecret}
          onUpdateTgToken={setTgToken}
          open={connectChannel !== null}
          pairing={pairing}
          tgState={tgState}
          tgToken={tgToken}
        />

        <ChannelDetailModal
          bindings={detailBindings}
          meta={detailMeta}
          onClose={() => setDetailChannel(null)}
          onOpenSession={onOpenSession}
          onRemovePeer={(peer) => removeRemotePeer(peer.channel, peer.peerId).then(() => reload())}
          open={detailChannel !== null}
          peers={detailPeers}
          sessions={sessions}
        />
      </div>

    </div>

  );
}

function WechatConnectPanel({
  mode,
  pairing,
}: {
  mode: "connect" | "reconnect";
  pairing: Pairing;
}) {
  if (pairing.kind === "loading") {
    return (
      <div className="flex items-center gap-2 rounded-lg border border-border bg-background px-3 py-4 text-sm text-foreground-muted">
        <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
        正在获取二维码…
      </div>
    );
  }

  if (pairing.kind === "qr") {
    return (
      <div className="grid justify-items-center gap-3 rounded-lg border border-border bg-background p-5 text-center">
        {pairing.image ? (
          <img
            src={pairing.image}
            alt="微信绑定二维码"
            className="h-44 w-44 rounded-lg border border-border-subtle bg-white p-2"
          />
        ) : (
          <div className="max-h-44 min-w-0 overflow-auto break-all rounded-lg border border-border-subtle bg-card p-3 text-xs text-foreground-secondary">
            {pairing.code}
          </div>
        )}
        <div className="flex min-w-0 flex-col items-center justify-center gap-2">
          <Badge tone={pairing.phase === "scanned" ? "running" : "info"} className="w-fit">
            {PHASE_LABEL[pairing.phase]}
          </Badge>
          <p className="text-xs leading-5 text-foreground-muted">请在手机端完成确认。二维码过期后可重新获取。</p>
        </div>
      </div>
    );
  }

  if (pairing.kind === "done") {
    return (
      <Message tone="success" className="text-xs leading-5">
        绑定成功。现在可以在微信里发消息试试。
      </Message>
    );
  }

  if (pairing.kind === "error") {
    return (
      <Message tone="danger" className="text-xs leading-5">
        {pairing.message}
      </Message>
    );
  }

  return (
    <div className="rounded-lg border border-dashed border-border-subtle bg-background px-3 py-4">
      <p className="text-xs leading-5 text-foreground-muted">
        {mode === "reconnect"
          ? "当前连接异常。可重新获取二维码完成连接。"
          : "点击右侧连接按钮获取二维码。连接后即可在微信里收发消息。"}
      </p>
    </div>
  );
}
