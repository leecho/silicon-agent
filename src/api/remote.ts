import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface RemoteChannelConfig {
  channel: string;
  enabled: boolean;
  status: "disconnected" | "connecting" | "connected" | "paused" | "error";
  configJson: string | null;
  lastError: string | null;
  updatedAt: string;
}

export interface AllowedPeer {
  channel: string;
  peerId: string;
  label: string | null;
  createdAt: string;
}

export interface RemoteBinding {
  channel: string;
  peerId: string;
  account: string | null;
  accountName: string | null;
  sessionId: string;
  contextToken: string | null;
  pendingKind: string | null;
  pendingPayload: string | null;
  updatedAt: string;
}

export interface QrCode {
  qrcode: string;
  /** 待编码的深链内容（前端生成二维码图片让用户用微信扫）。 */
  qrContent: string;
}

export type PairingPhase = "qr" | "scanned" | "confirmed" | "expired" | "error";

export interface PairingEvent {
  channel: string;
  phase: PairingPhase;
  qrcode?: string;
  qrContent?: string | null;
  message?: string | null;
}

export async function listRemoteChannels(): Promise<RemoteChannelConfig[]> {
  return await invoke<RemoteChannelConfig[]>("list_remote_channels");
}

export async function listRemoteAllowlist(): Promise<AllowedPeer[]> {
  return await invoke<AllowedPeer[]>("list_remote_allowlist");
}

export async function listRemoteBindings(): Promise<RemoteBinding[]> {
  return await invoke<RemoteBinding[]>("list_remote_bindings");
}

export async function removeRemotePeer(channel: string, peerId: string): Promise<void> {
  await invoke("remove_remote_peer", { channel, peerId });
}

/** 暂停远程 channel：保留配置和密钥，但停止当前运行时 connector。 */
export async function pauseRemoteChannel(channel: string): Promise<void> {
  await invoke("pause_remote_channel", { channel });
}

/** 恢复远程 channel：复用既有配置和密钥重新启动 connector。 */
export async function resumeRemoteChannel(channel: string): Promise<void> {
  await invoke("resume_remote_channel", { channel });
}

/** 解除远程 channel：停止 connector，清除配置和 secret。 */
export async function disconnectRemoteChannel(channel: string): Promise<void> {
  await invoke("disconnect_remote_channel", { channel });
}

/** 发起微信扫码配对，返回二维码（前端展示）。后续状态走 remote_pairing_event。 */
export async function beginWechatPairing(): Promise<QrCode> {
  return await invoke<QrCode>("begin_remote_wechat_pairing");
}

/** 连接 Telegram：保存 BotFather token + 启用 + 立即启动 connector。 */
export async function connectTelegram(token: string): Promise<void> {
  await invoke("connect_remote_telegram", { token });
}

/** 连接钉钉：保存 AppKey/AppSecret（自建应用）+ 启用 + 立即启动 Stream connector。 */
export async function connectDingtalk(appKey: string, appSecret: string): Promise<void> {
  await invoke("connect_remote_dingtalk", { appKey, appSecret });
}

/** 连接飞书：保存 AppID/AppSecret（自建应用）+ 启用 + 立即启动长连接 connector。 */
export async function connectFeishu(appId: string, appSecret: string): Promise<void> {
  await invoke("connect_remote_feishu", { appId, appSecret });
}

/** 订阅配对状态事件。返回 unlisten。 */
export async function onPairingEvent(cb: (e: PairingEvent) => void) {
  return await listen<PairingEvent>("remote_pairing_event", (event) => cb(event.payload));
}
