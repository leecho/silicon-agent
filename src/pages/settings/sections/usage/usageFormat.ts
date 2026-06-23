/** 千分位/紧凑格式化 token 数。 */
export function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

/** 百分比（0..1 → "xx.x%"）。 */
export function formatPercent(ratio: number): string {
  if (!isFinite(ratio) || ratio <= 0) return "0%";
  const pct = ratio * 100;
  if (pct < 0.1) return "<0.1%";
  return `${pct.toFixed(1)}%`;
}

/** epoch 秒字符串 → 本地 "MM-DD HH:mm"。 */
export function formatTs(ts: string): string {
  const sec = Number(ts);
  if (!isFinite(sec) || sec <= 0) return ts;
  const d = new Date(sec * 1000);
  const pad = (x: number) => String(x).padStart(2, "0");
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** 会话标题回退：无标题时用 sessionId 短码。 */
export function sessionLabel(sessionId: string, title: string): string {
  if (title && title.trim() !== "") return title;
  return sessionId.length > 8 ? `${sessionId.slice(0, 8)}…` : sessionId;
}

/** 调色板（语义色 token，循环取色）。返回 CSS 颜色（Tailwind 任意值需具体色，这里用 CSS 变量）。 */
export const USAGE_PALETTE = [
  "rgb(26 115 232)", // action primary 近似
  "rgb(217 119 6)", // amber
  "rgb(16 185 129)", // jade
  "rgb(139 92 246)", // violet
  "rgb(236 72 153)", // pink
  "rgb(100 116 139)", // slate
];

/** 稳定地按 key 取色（同 model 同色）。 */
export function pickColor(key: string): string {
  let hash = 0;
  for (let i = 0; i < key.length; i += 1) {
    hash = (hash * 31 + key.charCodeAt(i)) >>> 0;
  }
  return USAGE_PALETTE[hash % USAGE_PALETTE.length];
}

/** 生成最近 days 天的本地日期序列（含今天），格式 YYYY-MM-DD。 */
export function recentDates(days: number): string[] {
  const out: string[] = [];
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  for (let i = days - 1; i >= 0; i -= 1) {
    const d = new Date(today.getTime() - i * 86_400_000);
    const pad = (x: number) => String(x).padStart(2, "0");
    out.push(`${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`);
  }
  return out;
}
