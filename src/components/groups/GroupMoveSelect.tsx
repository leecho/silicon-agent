import type { Group } from "../../types";

/** 行内「移到分组」下拉：未分组 + 各分组。值为 group id；空=未分组。 */
export function GroupMoveSelect({
  groups,
  value,
  onChange,
}: {
  groups: Group[];
  value?: string | null;
  onChange: (groupId: string | null) => void;
}) {
  return (
    <select
      value={value ?? ""}
      onChange={(e) => onChange(e.target.value || null)}
      onClick={(e) => e.stopPropagation()}
      title="移到分组"
      className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground-secondary outline-none focus:border-primary"
    >
      <option value="">未分组</option>
      {groups.map((g) => (
        <option key={g.id} value={g.id}>{g.name}</option>
      ))}
    </select>
  );
}
