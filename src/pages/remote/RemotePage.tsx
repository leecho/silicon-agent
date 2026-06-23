import type { InputHTMLAttributes, ReactNode } from "react";
import { useEffect, useMemo, useState } from "react";
import {
  Bot,
  CheckCircle2,
  CircleSlash,
  KeyRound,
  Loader2,
  MessageSquare,
  QrCode,
  Send,
  ShieldCheck,
  Smartphone,
  Trash2,
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
import { Badge, Button, Message, Modal, ModalHeader, useMessages } from "../../components/ui";

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
    description: "扫码绑定后即可在微信里与本地 agent 对话。",
    icon: <Smartphone className="h-5 w-5" aria-hidden="true" />,
  },
  {
    id: "telegram",
    title: "Telegram",
    description: "在 BotFather 创建 bot，填入 token 后连接。",
    icon: <Send className="h-5 w-5" aria-hidden="true" />,
  },
  {
    id: "dingtalk",
    title: "钉钉",
    description: "创建企业自建应用并开启 Stream 模式。",
    icon: <MessageSquare className="h-5 w-5" aria-hidden="true" />,
  },
  {
    id: "feishu",
    title: "飞书 / Lark",
    description: "创建企业自建应用并开启长连接订阅「接收消息」。",
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

function RemoteRow({
  children,
  connectBusy,
  lifecycleBusy,
  meta,
  onConnect,
  onDetails,
  onDisconnect,
  onPause,
  onResume,
  status,
}: {
  children: ReactNode;
  connectBusy?: boolean;
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
        <Button tone="primary" disabled>
          <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
          连接中…
        </Button>
      );
    }

    if (status === "disconnected") {
      return (
        <Button tone="primary" onClick={onConnect}>
          {meta.id === "wechat" && <QrCode className="h-4 w-4" aria-hidden="true" />}
          连接
        </Button>
      );
    }

    if (status === "connected") {
      return (
        <>
          <Button tone="secondary" onClick={onPause} disabled={lifecycleBusy === "pause"}>
            {lifecycleBusy === "pause" && <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />}
            {lifecycleBusy === "pause" ? "暂停中…" : "暂停"}
          </Button>
          <Button tone="danger" onClick={onDisconnect} disabled={lifecycleBusy === "disconnect"}>
            {lifecycleBusy === "disconnect" && <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />}
            {lifecycleBusy === "disconnect" ? "解除中…" : "解除连接"}
          </Button>
        </>
      );
    }

    if (status === "paused") {
      return (
        <>
          <Button tone="primary" onClick={onResume} disabled={lifecycleBusy === "resume"}>
            {lifecycleBusy === "resume" && <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />}
            {lifecycleBusy === "resume" ? "恢复中…" : "恢复连接"}
          </Button>
          <Button tone="danger" onClick={onDisconnect} disabled={lifecycleBusy === "disconnect"}>
            {lifecycleBusy === "disconnect" && <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />}
            {lifecycleBusy === "disconnect" ? "解除中…" : "解除连接"}
          </Button>
        </>
      );
    }

    return (
      <>
        <Button tone="primary" onClick={onConnect}>
          重新连接
        </Button>
        <Button tone="danger" onClick={onDisconnect} disabled={lifecycleBusy === "disconnect"}>
          {lifecycleBusy === "disconnect" && <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />}
          {lifecycleBusy === "disconnect" ? "解除中…" : "解除连接"}
        </Button>
      </>
    );
  }

  return (
    <section className="rounded-lg border border-border bg-surface p-4">
      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-start">
        <div className="flex min-w-0 gap-3">
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-card text-foreground-secondary">
            {meta.icon}
          </span>
          <div className="min-w-0">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <h3 className="truncate text-sm font-semibold text-foreground">{meta.title}</h3>
              <Badge tone={statusTone(status)}>{statusLabel(status)}</Badge>
            </div>
            <p className="mt-1 text-xs leading-5 text-foreground-muted">{meta.description}</p>
          </div>
        </div>

        <div className="flex flex-wrap justify-end gap-2">
          {renderLifecycleActions()}
          <Button tone="secondary" onClick={onDetails}>
            详情
          </Button>
        </div>
      </div>

      <div className="mt-4 border-t border-border-subtle pt-4">{children}</div>
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
    <Modal className="w-[400px]" open={open} onClose={onClose} title={`${meta.title} 连接`}>
      <ModalHeader onClose={onClose}>
        <div className="flex min-w-0 items-center gap-3">
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-card text-foreground-secondary">
            {meta.icon}
          </span>
          <div className="min-w-0">
            <h2 className="truncate text-base font-semibold text-foreground">
              {reconnecting ? `重新连接 ${meta.title}` : `连接 ${meta.title}`}
            </h2>
            <p className="mt-1 text-xs text-foreground-muted">{meta.description}</p>
          </div>
        </div>
      </ModalHeader>

      <div className="mt-5">
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
              placeholder="123456:ABC-DEF…（BotFather token）"
            />
            <StateMessage state={tgState} successText="已连接。给你的 Telegram bot 发条消息试试。" />
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
            <div className="grid gap-3 md:grid-cols-1">
              <TextInput value={dtKey} onChange={(e) => onUpdateDtKey(e.target.value)} placeholder="AppKey" />
              <TextInput
                type="password"
                value={dtSecret}
                onChange={(e) => onUpdateDtSecret(e.target.value)}
                placeholder="AppSecret"
              />
            </div>
            <StateMessage state={dtState} successText="已连接。给你的钉钉 bot 发条消息试试。" />
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
            <div className="grid gap-3 md:grid-cols-1">
              <TextInput value={fsId} onChange={(e) => onUpdateFsId(e.target.value)} placeholder="App ID（cli_…）" />
              <TextInput
                type="password"
                value={fsSecret}
                onChange={(e) => onUpdateFsSecret(e.target.value)}
                placeholder="App Secret"
              />
            </div>
            <StateMessage state={fsState} successText="已连接。给你的飞书 bot 发条消息试试。" />
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
  onRemovePeer,
  open,
  peers,
}: {
  bindings: RemoteBinding[];
  meta: ChannelMeta | null;
  onClose: () => void;
  onRemovePeer: (peer: AllowedPeer) => void;
  open: boolean;
  peers: AllowedPeer[];
}) {
  if (!meta) return null;

  return (
    <Modal className="max-w-[400px]" open={open} onClose={onClose} title={`${meta.title} 详情`}>
      <ModalHeader onClose={onClose}>
        <div className="flex min-w-0 items-center gap-3">
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-card text-foreground-secondary">
            {meta.icon}
          </span>
          <div className="min-w-0">
            <h2 className="truncate text-base font-semibold text-foreground">{meta.title}</h2>
            <p className="mt-1 text-xs text-foreground-muted">查看该渠道的绑定联系人和会话映射。</p>
          </div>
        </div>
      </ModalHeader>

      <div className="mt-5 grid gap-4 grid-cols-1">
        <section>
          <div className="mb-2 flex items-center justify-between gap-3">
            <h3 className="text-sm font-semibold text-foreground">绑定联系人</h3>
            <Badge tone="info">{peers.length} 个</Badge>
          </div>
          {peers.length === 0 ? (
            <EmptyState icon={<CircleSlash className="h-5 w-5" aria-hidden="true" />}>
              暂无联系人。连接后给 bot 发一条消息即可自动认领。
            </EmptyState>
          ) : (
            <ul className="flex max-h-72 flex-col gap-2 overflow-auto pr-1">
              {peers.map((peer) => (
                <li
                  key={`${peer.channel}/${peer.peerId}`}
                  className="group flex min-w-0 items-center gap-3 rounded-lg border border-border-subtle bg-card px-3 py-2.5"
                >
                  <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-accent text-foreground-muted">
                    <ShieldCheck className="h-4 w-4" aria-hidden="true" />
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium text-foreground">
                      {peer.label ?? peer.peerId}
                    </div>
                    <div className="mt-1 truncate text-[11px] text-foreground-muted">{peer.peerId}</div>
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
        </section>

        <section>
          <div className="mb-2 flex items-center justify-between gap-3">
            <h3 className="text-sm font-semibold text-foreground">会话映射</h3>
            <Badge tone="info">{bindings.length} 条</Badge>
          </div>
          {bindings.length === 0 ? (
            <EmptyState icon={<KeyRound className="h-5 w-5" aria-hidden="true" />}>
              暂无会话映射。远程联系人开始对话后会在这里显示。
            </EmptyState>
          ) : (
            <ul className="flex max-h-72 flex-col gap-2 overflow-auto pr-1">
              {bindings.map((binding) => (
                <li
                  key={`${binding.channel}/${binding.peerId}`}
                  className="flex min-w-0 items-center gap-3 rounded-lg border border-border-subtle bg-card px-3 py-2.5"
                >
                  <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-accent text-foreground-muted">
                    <CheckCircle2 className="h-4 w-4" aria-hidden="true" />
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-xs font-medium text-foreground-secondary">
                      {binding.peerId}
                    </div>
                    <div className="mt-1 truncate text-[11px] text-foreground-muted">
                      会话 {binding.sessionId.slice(0, 12)}…
                    </div>
                  </div>
                  {binding.pendingKind && (
                    <Badge tone="warning" className="shrink-0">
                      待确认：{binding.pendingKind}
                    </Badge>
                  )}
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </Modal>
  );
}

/**
 * 远程接入设置：微信（ClawBot 扫码绑定）/ Telegram（BotFather token 连接），
 * 让你在 IM 里直接和本地 agent 对话。连接后第一个给 bot 发消息的人即被认作 owner。
 */
export function RemotePage() {
  const messages = useMessages();
  const [channels, setChannels] = useState<RemoteChannelConfig[]>([]);
  const [allowlist, setAllowlist] = useState<AllowedPeer[]>([]);
  const [bindings, setBindings] = useState<RemoteBinding[]>([]);
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
    const [nextChannels, nextAllowlist, nextBindings] = await Promise.all([
      listRemoteChannels(),
      listRemoteAllowlist(),
      listRemoteBindings(),
    ]);
    setChannels(nextChannels);
    setAllowlist(nextAllowlist);
    setBindings(nextBindings);
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
      title: "解除远程连接",
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
      title: "暂停远程连接",
      message: `确定暂停「${title}」吗？暂停后远程消息会收到暂停提示，不会进入 Agent；你可以稍后恢复连接。`,
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
            <h1 className="text-xl font-semibold text-foreground">IM渠道</h1>
            <p className="mt-1 text-xs text-foreground-muted">
              管理 IM 渠道连接、授权联系人和远程会话映射。
            </p>
          </div>
        </div>
        <section className="flex flex-col gap-3" aria-label="IM渠道">
        {CHANNELS.map((meta) => {
          const status = channelStatus(meta.id);
          const channelPeers = allowlist.filter((peer) => peer.channel === meta.id);
          const channelBindings = bindings.filter((binding) => binding.channel === meta.id);
          const config = channelConfig(meta.id);
          return (
            <RemoteRow
              key={meta.id}
              connectBusy={
                (meta.id === "wechat" && pairing.kind === "loading") ||
                (meta.id === "telegram" && tgState.kind === "connecting") ||
                (meta.id === "dingtalk" && dtState.kind === "connecting") ||
                (meta.id === "feishu" && fsState.kind === "connecting")
              }
              lifecycleBusy={lifecycleAction?.channel === meta.id ? lifecycleAction.kind : undefined}
              meta={meta}
              onConnect={() => setConnectChannel(meta.id)}
              onDetails={() => setDetailChannel(meta.id)}
              onDisconnect={() => disconnectChannel(meta.id)}
              onPause={() => pauseChannel(meta.id)}
              onResume={() => resumeChannel(meta.id)}
              status={status}
            >
              <div className="grid gap-2 text-xs text-foreground-muted md:grid-cols-4">
                <div>
                  <span className="font-medium text-foreground-secondary">绑定联系人</span>
                  <span className="ml-2">{channelPeers.length} 个</span>
                </div>
                <div>
                  <span className="font-medium text-foreground-secondary">会话映射</span>
                  <span className="ml-2">{channelBindings.length} 条</span>
                </div>
                <div className="min-w-0 truncate">
                  <span className="font-medium text-foreground-secondary">更新时间</span>
                  <span className="ml-2">{config?.updatedAt ?? "尚未连接"}</span>
                </div>
                <div className="min-w-0 truncate">
                  <span className="font-medium text-foreground-secondary">最近错误</span>
                  <span className="ml-2">{config?.lastError ?? "无"}</span>
                </div>
              </div>
            </RemoteRow>
          );
        })}

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
          onRemovePeer={(peer) => removeRemotePeer(peer.channel, peer.peerId).then(() => reload())}
          open={detailChannel !== null}
          peers={detailPeers}
        />
      </section>
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
      <div className="grid gap-3 rounded-lg border border-border bg-background p-3 sm:grid-cols-[auto_minmax(0,1fr)]">
        {pairing.image ? (
          <img
            src={pairing.image}
            alt="微信绑定二维码"
            className="h-36 w-36 rounded-lg border border-border-subtle bg-white p-2"
          />
        ) : (
          <div className="max-h-36 min-w-0 overflow-auto break-all rounded-lg border border-border-subtle bg-card p-3 text-xs text-foreground-secondary">
            {pairing.code}
          </div>
        )}
        <div className="flex min-w-0 flex-col justify-center gap-2">
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
        绑定成功。给你的微信 bot 发条消息试试。
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
          : "点击右侧连接按钮获取二维码。绑定后，第一个给 bot 发消息的联系人会自动成为 owner。"}
      </p>
    </div>
  );
}
