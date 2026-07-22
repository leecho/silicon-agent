import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ArrowUpRight, Bot, ChevronDown, Download, Folder, Plus, Search, Sparkles, UploadCloud } from "lucide-react";
import {
  createGroup,
  deleteExpert,
  deleteGroup,
  importExpertFromPath,
  listGroups,
  listManageableExperts,
  pickDirectory,
  renameGroup,
  setExpertGroup,
  toggleExpert,
} from "../../api";
import { avatarEmoji } from "../../lib/avatar";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import { GroupFilterBar } from "../../components/groups/GroupFilterBar";
import { DropdownMenu, type DropdownMenuAnchor, type DropdownMenuEntry } from "../../components/ui";
import { useSession } from "../../components/session/SessionProvider";
import { useMessages } from "../../components/ui/MessageProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { Switch } from "../../components/ui/Switch";
import { ExpertBuilderModal } from "./ExpertBuilderModal";
import { ExpertDetailDrawer } from "./ExpertDetailDrawer";
import { OwnerGroupTitle } from "../extensions/OwnerGroupTitle";
import type { ExpertSummary, Group } from "../../types";

// 「AI 创建」入口注入 composer 的提示词：引导 AI 走 create-expert 技能，问清后调 install_expert 登记。
const CREATE_AGENT_PROMPT =
  "帮我创建一个专家（助手角色）。先问我：这个专家要帮我干什么类型的活、该怎么干、产出什么格式。" +
  "问清后按 create-expert 技能设计角色，把角色设定写进 system_prompt（身份+行事准则+产出格式），并给几条 quick_prompts。" +
  "设计敲定后，你必须调用 install_expert 工具来真正登记它——只在对话里描述方案不算创建。" +
  "登记会请求我确认；完成后它会出现在「专家」列表，可在会话里选用或编入团队。";

/**
 * 专家页：管理 agent 定义（用户自建/内置 + plugin 提供），按 owner 分「我的 / 来自插件」两组。
 * 它们可作会话人设、被派发、或在团队页用作成员。team 私有的 agent 不在此列（属团队内部组件）。
 *
 * `embedded`：作为「扩展」页的专家 Tab 内嵌时——隐藏自带 h1（与胶囊 Tab 标签重复）
 * 与内部「专家广场」子 Tab（广场已统一到「扩展 → 市场」Tab）。T106 §5.2。
 */
export function ExpertsPage({
  embedded = false,
}: { embedded?: boolean } = {}) {
  const messages = useMessages();
  const notifications = useNotifications();
  const { enterDraftWithContent, enterDraftWithExpert } = useSession();
  const [agents, setExperts] = useState<ExpertSummary[]>([]);
  const [builderOpen, setBuilderOpen] = useState(false);
  const [detailId, setDetailId] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [mineQuery, setMineQuery] = useState("");
  const [groups, setGroups] = useState<Group[]>([]);
  // null=全部；"ungrouped"=未分组；其余=group id。
  const [selectedGroup, setSelectedGroup] = useState<string | null>(null);

  async function reload() {
    try {
      setExperts(await listManageableExperts());
    } catch (err) {
      notifications.notify({ tone: "error", title: "加载失败", message: String(err) });
    }
  }

  async function reloadGroups() {
    try {
      setGroups(await listGroups("agent"));
    } catch (err) {
      notifications.notify({ tone: "error", title: "加载分组失败", message: String(err) });
    }
  }

  useEffect(() => {
    void reload();
    void reloadGroups();
  }, []);

  async function handleCreateGroup(name: string) {
    try {
      await createGroup("agent", name);
      await reloadGroups();
    } catch (err) {
      notifications.notify({ tone: "error", title: "新建分组失败", message: String(err) });
    }
  }
  async function handleRenameGroup(id: string, name: string) {
    try {
      await renameGroup(id, name);
      await reloadGroups();
    } catch (err) {
      notifications.notify({ tone: "error", title: "重命名失败", message: String(err) });
    }
  }
  async function handleDeleteGroup(g: Group) {
    const ok = await messages.confirm({
      title: "删除分组",
      message: `确定删除分组「${g.name}」吗？组内专家不会被删除，只会变成未分组。`,
      tone: "warning",
      confirmText: "删除",
    });
    if (!ok) return;
    try {
      await deleteGroup(g.id, "agent");
      if (selectedGroup === g.id) setSelectedGroup(null);
      await Promise.all([reloadGroups(), reload()]);
    } catch (err) {
      notifications.notify({ tone: "error", title: "删除分组失败", message: String(err) });
    }
  }
  async function handleMoveExpert(a: ExpertSummary, groupId: string | null) {
    try {
      await setExpertGroup(a.id, groupId);
      await reload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "移动失败", message: String(err) });
    }
  }

  const enabledCount = useMemo(() => agents.filter((a) => a.enabled).length, [agents]);
  const ungroupedCount = useMemo(() => agents.filter((a) => !a.groupId).length, [agents]);
  const countByGroup = useMemo(() => {
    const m: Record<string, number> = {};
    for (const a of agents) if (a.groupId) m[a.groupId] = (m[a.groupId] ?? 0) + 1;
    return m;
  }, [agents]);
  const filteredMineExperts = useMemo(() => {
    const q = mineQuery.trim().toLocaleLowerCase();
    return agents.filter((agent) => {
      if (selectedGroup === "ungrouped" && agent.groupId) return false;
      if (selectedGroup && selectedGroup !== "ungrouped" && agent.groupId !== selectedGroup) return false;
      if (!q) return true;
      return [
        agent.displayName,
        agent.name,
        agent.profession,
        agent.description,
      ]
        .filter(Boolean)
        .join(" ")
        .toLocaleLowerCase()
        .includes(q);
    });
  }, [agents, mineQuery, selectedGroup]);
  // owner 分组（T106 §5.3）：「我的」= 用户自建/装的；「来自插件」= 随插件带来的。
  const ownExperts = useMemo(
    () => filteredMineExperts.filter((a) => !a.pluginId),
    [filteredMineExperts],
  );
  const pluginExperts = useMemo(
    () => filteredMineExperts.filter((a) => a.pluginId),
    [filteredMineExperts],
  );

  async function handleImport() {
    if (importing) return;
    let path: string | null = null;
    try {
      path = await pickDirectory();
    } catch (err) {
      notifications.notify({ tone: "error", title: "选择目录失败", message: String(err) });
      return;
    }
    if (!path) return;
    setImporting(true);
    try {
      const agent = await importExpertFromPath(path);
      notifications.notify({
        tone: "success",
        title: "导入成功",
        message: `已导入专家「${agent.displayName || agent.name}」及其私有技能。`,
      });
      await reload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "导入失败", message: String(err) });
    } finally {
      setImporting(false);
    }
  }

  async function handleToggle(a: ExpertSummary) {
    try {
      await toggleExpert(a.id, !a.enabled);
      await reload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "操作失败", message: String(err) });
    }
  }

  async function handleDelete(a: ExpertSummary) {
    const ok = await messages.confirm({
      title: "删除专家",
      message: `确定删除专家「${a.displayName || a.name}」吗？操作不可撤销。`,
      tone: "warning",
      confirmText: "删除",
    });
    if (!ok) return;
    try {
      await deleteExpert(a.id);
      await reload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "删除失败", message: String(err) });
    }
  }

  return (
    <div className="h-full overflow-auto px-6 py-3 text-sm">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-5 flex items-center justify-between gap-4">
          <div>
            {!embedded && <h1 className="text-xl font-semibold text-foreground">专家</h1>}
            <p className="mt-1 text-sm text-foreground">
              发现、创建和管理助手角色，可在对话中选用，也可编入团队协作。
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button tone="primary" onClick={() => enterDraftWithContent(CREATE_AGENT_PROMPT)}>
              <Sparkles className="h-4 w-4" aria-hidden="true" />
              AI 创建
            </Button>
            <Button tone="secondary" disabled={importing} onClick={() => void handleImport()}>
              <Plus className="h-4 w-4" aria-hidden="true" />
              {importing ? "正在安装…" : "安装"}
            </Button>
            {/* <Button tone="primary" onClick={() => setBuilderOpen(true)}>
              <Plus className="h-4 w-4" aria-hidden="true" />
              新建专家
            </Button> */}
          </div>
        </div>

        {agents.length === 0 ? (
          <EmptyState onCreate={() => enterDraftWithContent(CREATE_AGENT_PROMPT)} />
        ) : (
            <>
              <MineExpertSearch
                onQueryChange={setMineQuery}
                query={mineQuery}
              />
              <GroupFilterBar
                groups={groups}
                selected={selectedGroup}
                onSelect={setSelectedGroup}
                total={agents.length}
                ungroupedCount={ungroupedCount}
                countByGroup={countByGroup}
                onCreate={handleCreateGroup}
                onRename={handleRenameGroup}
                onDelete={handleDeleteGroup}
              />
              {filteredMineExperts.length === 0 ? (
                <div className="rounded-lg border border-dashed border-border px-4 py-12 text-center text-sm text-foreground-muted">
                  没有匹配的专家
                </div>
              ) : (
                <div className="flex flex-col gap-6">
                  {ownExperts.length > 0 && (
                    <section>
                      <OwnerGroupTitle title="我的" count={ownExperts.length} />
                      <ExpertList
                        agents={ownExperts}
                        groups={groups}
                        onDelete={handleDelete}
                        onMove={handleMoveExpert}
                        onOpen={setDetailId}
                        onToggle={handleToggle}
                        onUse={(expertId) => enterDraftWithExpert(expertId)}
                      />
                    </section>
                  )}
                  {pluginExperts.length > 0 && (
                    <section>
                      <OwnerGroupTitle
                        title="来自插件"
                        count={pluginExperts.length}
                        hint="随插件安装，卸载插件即一并移除"
                      />
                      <ExpertList
                        agents={pluginExperts}
                        groups={groups}
                        onOpen={setDetailId}
                        onUse={(expertId) => enterDraftWithExpert(expertId)}
                        readOnly
                      />
                    </section>
                  )}
                </div>
              )}
          </>
        )}
      </div>

      <ExpertBuilderModal
        open={builderOpen}
        onClose={() => setBuilderOpen(false)}
        onCreated={() => {
          setBuilderOpen(false);
          void reload();
        }}
      />
      <ExpertDetailDrawer expertId={detailId} onClose={() => setDetailId(null)} />
    </div>
  );
}


function MineExpertSearch({
  onQueryChange,
  query,
}: {
  onQueryChange: (query: string) => void;
  query: string;
}) {
  return (
    <div className="mb-4 flex flex-wrap items-center gap-2">
      <div className="relative min-w-[220px] flex-1">
        <Search className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-foreground-muted" aria-hidden="true" />
        <input
          className="w-full rounded-md border border-border bg-background py-2 pl-8 pr-3 text-sm text-foreground outline-none focus:border-primary"
          placeholder="搜索我的专家名称、职业或描述"
          value={query}
          onChange={(event) => onQueryChange(event.target.value)}
        />
      </div>
    </div>
  );
}

/**
 * 专家列表。**行式连接列表**（与「插件」页同构）：一行一个，整块一个边框、行间分隔线。
 */
function ExpertList({
  agents,
  groups,
  onDelete,
  onMove,
  onOpen,
  onToggle,
  onUse,
  readOnly = false,
}: {
  agents: ExpertSummary[];
  groups: Group[];
  onDelete?: (agent: ExpertSummary) => void | Promise<void>;
  onMove?: (agent: ExpertSummary, groupId: string | null) => void | Promise<void>;
  onOpen: (expertId: string) => void;
  onToggle?: (agent: ExpertSummary) => void | Promise<void>;
  onUse: (expertId: string) => void;
  /** 只读：插件带来的专家随插件启停，不可单独开关/删除（T53 / T106 §5.2）。 */
  readOnly?: boolean;
}) {
  return (
    <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
      {agents.map((agent, index) => (
        <ExpertRow
          agent={agent}
          groups={groups}
          key={agent.id}
          last={index === agents.length - 1}
          onDelete={onDelete}
          onMove={onMove}
          onOpen={onOpen}
          onToggle={onToggle}
          onUse={onUse}
          readOnly={readOnly}
        />
      ))}
    </ul>
  );
}

function ExpertRow({
  agent,
  groups,
  last,
  onDelete,
  onMove,
  onOpen,
  onToggle,
  onUse,
  readOnly = false,
}: {
  agent: ExpertSummary;
  groups: Group[];
  last: boolean;
  onDelete?: (agent: ExpertSummary) => void | Promise<void>;
  onMove?: (agent: ExpertSummary, groupId: string | null) => void | Promise<void>;
  onOpen: (expertId: string) => void;
  onToggle?: (agent: ExpertSummary) => void | Promise<void>;
  onUse: (expertId: string) => void;
  readOnly?: boolean;
}) {
  return (
    <li
      className={`group flex items-center gap-3.5 px-4 py-4 transition-colors hover:bg-primary/5 ${
        last ? "" : "border-b border-border-subtle"
      }`}
    >
      <ExpertAvatar agent={agent} />

      {/* 标题行不整块套 <button>：分组下拉本身是按钮，嵌套 button 是非法 HTML。 */}
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => onOpen(agent.id)}
            className="min-w-0 truncate text-left font-semibold text-foreground"
          >
            {agent.displayName || agent.name}
          </button>
          {agent.profession && <Badge tone="info">{agent.profession}</Badge>}
          {!readOnly && onMove && (
            <ExpertGroupDropdown
              groups={groups}
              value={agent.groupId}
              onChange={(gid) => void onMove(agent, gid)}
            />
          )}
        </div>
        {agent.description && (
          <button
            type="button"
            onClick={() => onOpen(agent.id)}
            className="block w-full text-left"
          >
            <p className="mt-0.5 line-clamp-1 text-xs text-foreground-secondary">
              {agent.description}
            </p>
          </button>
        )}
      </div>

      <div className="flex shrink-0 items-center gap-3">
        {/* 悬浮才出现：一屏几十行，常显的话按钮比内容还抢眼。 */}
        <div className="pointer-events-none flex items-center gap-1 opacity-0 transition group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100">
          <button
            type="button"
            onClick={() => onUse(agent.id)}
            className="inline-flex shrink-0 items-center gap-1 rounded-md bg-primary px-2 py-1 text-xs font-medium text-primary-foreground transition hover:opacity-90"
          >
            <ArrowUpRight className="h-3.5 w-3.5" aria-hidden="true" />
            使用
          </button>
          {!readOnly && agent.source === "user" && onDelete && (
            <button
              type="button"
              onClick={() => void onDelete(agent)}
              className="rounded-md px-2 py-1 text-xs text-foreground-muted transition hover:bg-accent hover:text-destructive"
            >
              删除
            </button>
          )}
        </div>

        {readOnly ? (
          // 插件带来的专家随插件启停 —— 给状态，不给开关，否则用户点了却没反应。
          <span className="text-xs text-foreground-muted">
            {agent.enabled ? "已启用" : "已停用"} · 随插件
          </span>
        ) : (
          <Switch checked={agent.enabled} onChange={() => void onToggle?.(agent)} />
        )}
      </div>
    </li>
  );
}

function ExpertGroupDropdown({
  groups,
  onChange,
  value,
}: {
  groups: Group[];
  onChange: (groupId: string | null) => void;
  value?: string | null;
}) {
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState(false);
  const [anchorRect, setAnchorRect] = useState<DropdownMenuAnchor | null>(null);
  const selectedGroup = groups.find((group) => group.id === value);
  const label = selectedGroup?.name ?? "未分组";
  const items: DropdownMenuEntry[] = [
    {
      id: "__ungrouped__",
      icon: Folder,
      label: "未分组",
      selected: !value,
      onSelect: () => onChange(null),
    },
    ...groups.map((group): DropdownMenuEntry => ({
      id: group.id,
      icon: Folder,
      label: group.name,
      selected: group.id === value,
      onSelect: () => onChange(group.id),
    })),
  ];

  function toggleMenu() {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (!rect) return;
    setAnchorRect({
      bottom: rect.bottom,
      left: rect.left,
      right: rect.right,
      top: rect.top,
    });
    setOpen((current) => !current);
  }

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        className="inline-flex max-w-[132px] shrink-0 rounded-full text-xs outline-none transition hover:opacity-85 focus:ring-1 focus:ring-ring"
        onClick={(event) => {
          event.stopPropagation();
          toggleMenu();
        }}
      >
        <Badge tone={selectedGroup ? "info" : "neutral"} className="inline-flex max-w-full min-w-0 items-center gap-1">
          <span className="min-w-0 truncate">{label}</span>
          <ChevronDown className="h-3 w-3 shrink-0" aria-hidden="true" />
        </Badge>
      </button>
      {open && (
        <DropdownMenu
          align="start"
          anchorElement={triggerRef.current}
          anchorRect={anchorRect}
          items={items}
          onClose={() => setOpen(false)}
          placement="bottom"
          width={168}
        />
      )}
    </>
  );
}

function ExpertAvatar({ agent }: { agent: ExpertSummary }) {
  const emoji = avatarEmoji(agent.avatar);
  return (
    <div
      className={`grid h-10 w-10 shrink-0 place-items-center rounded-lg border border-border bg-background text-[18px] shadow-sm transition-colors ${
        agent.enabled ? "text-primary" : "text-foreground-muted"
      }`}
    >
      {emoji ? <span aria-hidden="true">{emoji}</span> : <Bot className="h-5 w-5" aria-hidden="true" />}
    </div>
  );
}

function EmptyState({ onCreate }: { onCreate: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-border bg-surface/40 py-16 text-foreground-muted">
      <div className="grid h-12 w-12 place-items-center rounded-full bg-muted">
        <Bot className="h-6 w-6" aria-hidden="true" />
      </div>
      <p className="text-sm">你还没有创建专家</p>
      <Button tone="outline" onClick={onCreate}>
        <Plus className="h-4 w-4" aria-hidden="true" />
        新建专家
      </Button>
    </div>
  );
}
