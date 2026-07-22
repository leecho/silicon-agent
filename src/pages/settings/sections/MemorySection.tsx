import { useEffect, useMemo, useState } from "react";
import { Brain, Pencil, Plus, Search, Sparkles, Trash2, UserCircle } from "lucide-react";
import {
  addMemory,
  addScopedMemory,
  clearMemories,
  curateMemories,
  deleteMemory,
  getMemoryProfile,
  listMemories,
  listScopedMemories,
  setMemoryProfile,
  updateMemory,
} from "../../../api";
import type { MemoryScopeKind } from "../../../api";
import { Button, Modal, ModalHeader, Tooltip } from "../../../components/ui";
import type { Memory } from "../../../types";

/** 作用域：不传=全局（设置页，含画像/整理）；传则为项目/智能体层（仅事实 CRUD+搜索）。 */
export interface MemoryScopeProp {
  kind: MemoryScopeKind;
  id: string;
  label?: string;
}

/** 把 ISO 时间字符串格式化为本地可读时间；解析失败时回退原值。 */
function formatCreatedAt(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

/** 待删除目标：单条记忆，或"清空全部"。 */
type PendingDelete = { kind: "one"; memory: Memory } | { kind: "all" };

/** 编辑器目标：新增一条，或编辑某条。 */
type Editor = { kind: "add" } | { kind: "edit"; memory: Memory };

/** 长期记忆 section：查看、搜索、新增、编辑与删除模型记录的长期记忆。
 * 无 scope=全局（设置页，含画像/整理/清空）；有 scope=项目/智能体层（仅事实 CRUD+搜索）。 */
export function MemorySection({ scope }: { scope?: MemoryScopeProp } = {}) {
  const scoped = scope != null;
  const [memories, setMemories] = useState<Memory[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [pending, setPending] = useState<PendingDelete | null>(null);
  const [editor, setEditor] = useState<Editor | null>(null);
  const [draft, setDraft] = useState("");
  const [working, setWorking] = useState(false);
  // 用户画像（Tier1 常驻注入）。
  const [profile, setProfile] = useState("");
  const [profileDraft, setProfileDraft] = useState("");
  const [profileSaving, setProfileSaving] = useState(false);
  const [profileEditing, setProfileEditing] = useState(false);
  const [curating, setCurating] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  function reload() {
    setLoading(true);
    setError(null);
    const list = scope ? listScopedMemories(scope.kind, scope.id) : listMemories();
    list
      .then(setMemories)
      .catch((err) => setError(String(err)))
      .finally(() => setLoading(false));
    // 画像仅全局层。
    if (!scoped) {
      getMemoryProfile()
        .then((p) => {
          setProfile(p);
          setProfileDraft(p);
        })
        .catch(() => {});
    }
  }

  // scope 变化（切换不同项目/智能体）时重载。
  useEffect(reload, [scope?.kind, scope?.id]);

  function openProfileEdit() {
    setProfileDraft(profile);
    setProfileEditing(true);
  }

  async function saveProfile() {
    setProfileSaving(true);
    setError(null);
    try {
      const next = profileDraft.trim();
      await setMemoryProfile(next);
      setProfile(next);
      setProfileEditing(false);
    } catch (err) {
      setError(String(err));
    } finally {
      setProfileSaving(false);
    }
  }

  async function runCuration() {
    setCurating(true);
    setError(null);
    setNotice(null);
    try {
      const r = await curateMemories();
      setNotice(
        r.ran
          ? `整理完成：事实 ${r.factsBefore} → ${r.factsAfter}${r.profileUpdated ? "，画像已更新" : ""}。`
          : `事实较少（${r.factsBefore} 条），暂无需整理。`,
      );
      reload();
    } catch (err) {
      setError(String(err));
    } finally {
      setCurating(false);
    }
  }

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return memories;
    return memories.filter((m) => m.content.toLowerCase().includes(q));
  }, [memories, query]);

  function openAdd() {
    setEditor({ kind: "add" });
    setDraft("");
  }

  function openEdit(memory: Memory) {
    setEditor({ kind: "edit", memory });
    setDraft(memory.content);
  }

  async function saveEditor() {
    if (!editor) return;
    const content = draft.trim();
    if (!content) return;
    setWorking(true);
    try {
      if (editor.kind === "add") {
        await (scope ? addScopedMemory(scope.kind, scope.id, content) : addMemory(content));
      } else {
        await updateMemory(editor.memory.id, content);
      }
      setEditor(null);
      reload();
    } catch (err) {
      setError(String(err));
    } finally {
      setWorking(false);
    }
  }

  async function confirmDelete() {
    if (!pending) return;
    setWorking(true);
    try {
      if (pending.kind === "all") {
        await clearMemories();
      } else {
        await deleteMemory(pending.memory.id);
      }
      setPending(null);
      reload();
    } catch (err) {
      setError(String(err));
    } finally {
      setWorking(false);
    }
  }

  return (
    <section className="flex flex-col gap-5" aria-label="长期记忆">
      <p className="text-sm text-foreground-muted">
        {scoped
          ? scope?.kind === "project"
            ? "本项目的长期记忆，项目内所有线程与成员共享，相关时自动注入上下文。也可在此手动新增或编辑。"
            : "该智能体的私有长期记忆，跨会话保留，相关时自动注入其上下文。也可在此手动新增或编辑。"
          : "模型通过 remember 工具记录的长期记忆，按与当前任务的相关性检索注入上下文。也可在此手动新增或编辑。"}
      </p>

      {/* 用户画像（Tier1 常驻）——仅全局层，默认只读，点「编辑」弹窗修改。 */}
      {!scoped && (
        <div className="rounded-2xl border border-border bg-surface p-4">
          <div className="mb-2 flex items-center gap-2">
            <UserCircle className="h-4 w-4 text-foreground-muted" aria-hidden="true" />
            <h3 className="text-sm font-medium text-foreground">用户画像</h3>
            <span className="text-xs text-foreground-muted">（稳定的偏好/背景，每次对话常驻注入）</span>
            <Tooltip content="编辑用户画像">
              <button
                type="button"
                aria-label="编辑用户画像"
                onClick={openProfileEdit}
                className="ml-auto grid h-8 w-8 shrink-0 place-items-center rounded-lg text-foreground-muted transition hover:bg-background hover:text-foreground"
              >
                <Pencil className="h-4 w-4" aria-hidden="true" />
              </button>
            </Tooltip>
          </div>
          {profile.trim() ? (
            <p className="whitespace-pre-wrap text-sm leading-5 text-foreground [overflow-wrap:anywhere]">
              {profile}
            </p>
          ) : (
            <p className="text-sm text-foreground-muted">
              尚未设置用户画像，点右上角「编辑」补充稳定的偏好/背景。
            </p>
          )}
        </div>
      )}

      {notice && <p className="text-sm text-foreground-secondary">{notice}</p>}

      <div className="flex flex-wrap items-center gap-3">
        <div className="relative min-w-0 flex-1">
          <Search
            className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-foreground-muted"
            aria-hidden="true"
          />
          <input
            type="text"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索记忆内容…"
            aria-label="搜索记忆"
            className="h-10 w-full rounded-lg border border-border bg-background pl-9 pr-3 text-sm text-foreground outline-none transition placeholder:text-foreground-muted focus:border-ring"
          />
        </div>
        <Button tone="primary" className="shrink-0 px-4" onClick={openAdd}>
          <Plus className="h-4 w-4" aria-hidden="true" />
          新增记忆
        </Button>
        {!scoped && memories.length > 0 && (
          <Tooltip content="模型驱动地去重、合并事实并更新画像">
            <Button
              className="shrink-0 px-4"
              onClick={() => void runCuration()}
              disabled={curating}
            >
              <Sparkles className="h-4 w-4" aria-hidden="true" />
              {curating ? "整理中…" : "整理记忆"}
            </Button>
          </Tooltip>
        )}
        {!scoped && memories.length > 0 && (
          <Button
            tone="danger"
            className="shrink-0 px-4"
            onClick={() => setPending({ kind: "all" })}
          >
            <Trash2 className="h-4 w-4" aria-hidden="true" />
            清空全部
          </Button>
        )}
      </div>

      {error && <p className="text-sm text-destructive">操作失败：{error}</p>}
      {loading && <p className="text-sm text-foreground-muted">加载中…</p>}

      {!loading && memories.length === 0 && (
        <div className="rounded-2xl border border-border-subtle bg-surface  p-8 text-center">
          <Brain className="mx-auto h-6 w-6 text-foreground-muted" aria-hidden="true" />
          <p className="mt-2 text-sm font-medium text-foreground-secondary">暂无长期记忆</p>
        </div>
      )}

      {!loading && memories.length > 0 && filtered.length === 0 && (
        <p className="text-sm text-foreground-muted">没有匹配「{query}」的记忆。</p>
      )}

      {!loading && filtered.length > 0 && (
        <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
          {filtered.map((memory, i) => (
            <li
              key={memory.id}
              className={i === filtered.length - 1 ? "" : "border-b border-border-subtle"}
            >
              <div className="flex items-start gap-3 px-4 py-2.5 transition-colors hover:bg-accent">
                <div className="min-w-0 flex-1">
                  <p className="line-clamp-2 whitespace-pre-wrap text-sm leading-5 text-foreground [overflow-wrap:anywhere]">
                    {memory.content}
                  </p>
                  <p className="mt-0.5 text-xs leading-5 text-foreground-muted">
                    {formatCreatedAt(memory.createdAt)}
                  </p>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <Tooltip content="编辑记忆">
                    <button
                      type="button"
                      aria-label="编辑记忆"
                      onClick={() => openEdit(memory)}
                      className="grid h-8 w-8 place-items-center rounded-lg text-foreground-muted transition hover:bg-background hover:text-foreground"
                    >
                      <Pencil className="h-4 w-4" aria-hidden="true" />
                    </button>
                  </Tooltip>
                  <Tooltip content="删除记忆">
                    <button
                      type="button"
                      aria-label="删除记忆"
                      onClick={() => setPending({ kind: "one", memory })}
                      className="grid h-8 w-8 place-items-center rounded-lg text-foreground-muted transition hover:bg-background hover:text-destructive"
                    >
                      <Trash2 className="h-4 w-4" aria-hidden="true" />
                    </button>
                  </Tooltip>
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}

      {/* 用户画像编辑弹窗 */}
      <Modal
        open={profileEditing}
        onClose={() => (profileSaving ? undefined : setProfileEditing(false))}
        title="编辑用户画像"
        className="max-w-lg"
      >
        <ModalHeader onClose={() => setProfileEditing(false)}>
          <h2 className="text-base font-semibold text-foreground">编辑用户画像</h2>
        </ModalHeader>
        <textarea
          value={profileDraft}
          onChange={(event) => setProfileDraft(event.target.value)}
          rows={5}
          placeholder="例如：用户是 Rust 工程师，偏好简洁直接的回答…"
          aria-label="用户画像"
          className="mt-3 w-full resize-y rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition placeholder:text-foreground-muted focus:border-ring"
        />
        <div className="mt-5 flex justify-end gap-2">
          <Button onClick={() => setProfileEditing(false)} disabled={profileSaving}>
            取消
          </Button>
          <Button
            tone="primary"
            onClick={() => void saveProfile()}
            disabled={profileSaving || profileDraft.trim() === profile.trim()}
          >
            {profileSaving ? "保存中…" : "保存画像"}
          </Button>
        </div>
      </Modal>

      {/* 新增 / 编辑 弹窗 */}
      <Modal
        open={editor !== null}
        onClose={() => (working ? undefined : setEditor(null))}
        title={editor?.kind === "add" ? "新增记忆" : "编辑记忆"}
        className="max-w-lg"
      >
        <ModalHeader onClose={() => setEditor(null)}>
          <h2 className="text-base font-semibold text-foreground">
            {editor?.kind === "add" ? "新增记忆" : "编辑记忆"}
          </h2>
        </ModalHeader>
        <textarea
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          rows={5}
          placeholder="输入要长期记住的信息…"
          aria-label="记忆内容"
          className="mt-3 w-full resize-y rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition placeholder:text-foreground-muted focus:border-ring"
        />
        <div className="mt-5 flex justify-end gap-2">
          <Button onClick={() => setEditor(null)} disabled={working}>
            取消
          </Button>
          <Button
            tone="primary"
            onClick={() => void saveEditor()}
            disabled={working || draft.trim() === ""}
          >
            {working ? "保存中…" : "保存"}
          </Button>
        </div>
      </Modal>

      {/* 删除 / 清空确认弹窗 */}
      <Modal
        open={pending !== null}
        onClose={() => (working ? undefined : setPending(null))}
        title={pending?.kind === "all" ? "清空全部记忆" : "删除记忆"}
        className="max-w-md"
      >
        <ModalHeader onClose={() => setPending(null)}>
          <h2 className="text-base font-semibold text-foreground">
            {pending?.kind === "all" ? "清空全部记忆" : "删除这条记忆"}
          </h2>
        </ModalHeader>
        <p className="mt-3 text-sm text-foreground-secondary">
          {pending?.kind === "all"
            ? "将永久删除全部长期记忆，此操作不可撤销。"
            : "将永久删除这条长期记忆，此操作不可撤销。"}
        </p>
        {pending?.kind === "one" && (
          <p className="mt-3 max-h-32 overflow-auto whitespace-pre-wrap break-words rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground-muted">
            {pending.memory.content}
          </p>
        )}
        <div className="mt-5 flex justify-end gap-2">
          <Button onClick={() => setPending(null)} disabled={working}>
            取消
          </Button>
          <Button tone="danger" onClick={() => void confirmDelete()} disabled={working}>
            {working ? "删除中…" : "删除"}
          </Button>
        </div>
      </Modal>
    </section>
  );
}
