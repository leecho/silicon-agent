/**
 * 「扩展」页各 Tab 的 owner 分组小标题（T106 §5.3）。
 *
 * 用户面只说人话：「我的」/「我添加的」与「来自插件」。
 * 内部术语（散装 / owner 三态 / plugin_id）不出现在任何用户可见文案里。
 */
export function OwnerGroupTitle({
  count,
  hint,
  title,
}: {
  count: number;
  hint?: string;
  title: string;
}) {
  return (
    <div className="mb-3 flex items-baseline gap-2">
      <h2 className="text-sm font-semibold text-foreground">{title}</h2>
      <span className="text-xs text-foreground-muted">{count}</span>
      {hint && <span className="text-xs text-foreground-muted">· {hint}</span>}
    </div>
  );
}
