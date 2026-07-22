/** 把未来 epoch 秒格式化为「大约 N 小时内 / N 分钟内 / N 天内」。 */
export function relativeFromNow(epochSecs?: number | null): string {
  if (!epochSecs) return "未排期";
  const diff = epochSecs - Math.floor(Date.now() / 1000);
  if (diff <= 0) return "即将执行";
  const mins = Math.round(diff / 60);
  if (mins < 60) return `大约 ${mins} 分钟内`;
  const hours = Math.round(diff / 3600);
  if (hours < 48) return `大约 ${hours} 小时内`;
  const days = Math.round(diff / 86400);
  return `大约 ${days} 天内`;
}

/** epoch 秒 → 本地 "HH:MM:SS"。 */
export function formatClock(epochSecs?: number | null): string {
  if (!epochSecs) return "";
  return new Date(epochSecs * 1000).toLocaleTimeString();
}

/** 执行时长（秒，一位小数）。 */
export function durationSecs(start: number, end?: number | null): string {
  const e = end ?? Math.floor(Date.now() / 1000);
  return `${(e - start).toFixed(1)}s`;
}
