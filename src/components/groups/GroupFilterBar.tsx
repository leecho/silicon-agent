import { useState } from "react";
import { Check, Pencil, Plus, Trash2, X } from "lucide-react";
import type { Group } from "../../types";

/** 「我的」分组过滤条：全部 / 未分组 / 各分组 chips + 新建/重命名/删除。
 * 纯展示 + 回调，数据与持久化由父页负责。 */
export function GroupFilterBar({
  groups,
  selected,
  onSelect,
  total,
  ungroupedCount,
  countByGroup,
  onCreate,
  onRename,
  onDelete,
}: {
  groups: Group[];
  /** null=全部；"ungrouped"=未分组；其余=group id。 */
  selected: string | null;
  onSelect: (sel: string | null) => void;
  total: number;
  ungroupedCount: number;
  countByGroup: Record<string, number>;
  onCreate: (name: string) => void | Promise<void>;
  onRename: (id: string, name: string) => void | Promise<void>;
  onDelete: (group: Group) => void | Promise<void>;
}) {
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState("");

  async function submitNew() {
    const n = newName.trim();
    if (n) await onCreate(n);
    setNewName("");
    setCreating(false);
  }
  async function submitEdit(id: string) {
    const n = editName.trim();
    if (n) await onRename(id, n);
    setEditingId(null);
  }

  return (
    <div className="mb-4 flex flex-wrap items-center gap-1.5">
      <Chip active={selected === null} onClick={() => onSelect(null)}>
        全部 <span className="text-foreground-muted">{total}</span>
      </Chip>
      <Chip active={selected === "ungrouped"} onClick={() => onSelect("ungrouped")}>
        未分组 <span className="text-foreground-muted">{ungroupedCount}</span>
      </Chip>

      {groups.map((g) =>
        editingId === g.id ? (
          <span key={g.id} className="inline-flex items-center gap-1 rounded-full border border-primary bg-background px-2 py-0.5">
            <input
              autoFocus
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void submitEdit(g.id);
                if (e.key === "Escape") setEditingId(null);
              }}
              className="w-24 bg-transparent text-xs text-foreground outline-none"
            />
            <button type="button" onClick={() => void submitEdit(g.id)} className="text-primary"><Check className="h-3.5 w-3.5" /></button>
            <button type="button" onClick={() => setEditingId(null)} className="text-foreground-muted"><X className="h-3.5 w-3.5" /></button>
          </span>
        ) : (
          <span
            key={g.id}
            className={`group inline-flex items-center rounded-full border px-3 py-1 text-xs transition ${
              selected === g.id ? "border-primary bg-primary/10 text-primary" : "border-border-subtle text-foreground-secondary hover:bg-accent"
            }`}
          >
            <button type="button" onClick={() => onSelect(g.id)} className="inline-flex items-center gap-1">
              {g.name} <span className="text-foreground-muted">{countByGroup[g.id] ?? 0}</span>
            </button>
            <span className="ml-0 flex max-w-0 items-center overflow-hidden opacity-0 transition-all duration-150 group-focus-within:ml-1 group-focus-within:max-w-[44px] group-focus-within:opacity-100 group-hover:ml-1 group-hover:max-w-[44px] group-hover:opacity-100">
              <button
                type="button"
                title="重命名"
                onClick={() => { setEditingId(g.id); setEditName(g.name); }}
                className="grid h-4 w-4 shrink-0 place-items-center hover:text-primary"
              >
                <Pencil className="h-3 w-3" />
              </button>
              <button
                type="button"
                title="删除分组"
                onClick={() => void onDelete(g)}
                className="grid h-4 w-4 shrink-0 place-items-center hover:text-destructive"
              >
                <Trash2 className="h-3 w-3" />
              </button>
            </span>
          </span>
        ),
      )}

      {creating ? (
        <span className="inline-flex items-center gap-1 rounded-full border border-primary bg-background px-2 py-0.5">
          <input
            autoFocus
            value={newName}
            placeholder="分组名"
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void submitNew();
              if (e.key === "Escape") { setNewName(""); setCreating(false); }
            }}
            className="w-24 bg-transparent text-xs text-foreground outline-none"
          />
          <button type="button" onClick={() => void submitNew()} className="text-primary"><Check className="h-3.5 w-3.5" /></button>
          <button type="button" onClick={() => { setNewName(""); setCreating(false); }} className="text-foreground-muted"><X className="h-3.5 w-3.5" /></button>
        </span>
      ) : (
        <button
          type="button"
          onClick={() => setCreating(true)}
          className="inline-flex items-center gap-1 rounded-full border border-dashed border-border px-3 py-1 text-xs text-foreground-muted transition hover:border-primary hover:text-primary"
        >
          <Plus className="h-3.5 w-3.5" /> 新建分组
        </button>
      )}
    </div>
  );
}

function Chip({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`rounded-full border px-3 py-1 text-xs transition ${
        active ? "border-primary bg-primary/10 text-primary" : "border-border-subtle text-foreground-secondary hover:bg-accent"
      }`}
    >
      {children}
    </button>
  );
}
