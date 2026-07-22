import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ArrowUpRight, ChevronDown, Folder, Plus, Search, Sparkles, UploadCloud, Users } from "lucide-react";
import {
  createGroup,
  deleteGroup,
  deleteTeam,
  listGroups,
  listTeams,
  renameGroup,
  setTeamGroup,
  toggleTeam,
} from "../../api";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import { GroupFilterBar } from "../../components/groups/GroupFilterBar";
import { DropdownMenu, type DropdownMenuAnchor, type DropdownMenuEntry } from "../../components/ui";
import { useSession } from "../../components/session/SessionProvider";
import { useMessages } from "../../components/ui/MessageProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { Switch } from "../../components/ui/Switch";
import { TeamBuilderModal } from "./TeamBuilderModal";
import { TeamDetailDrawer } from "./TeamDetailDrawer";
import { TeamImportModal } from "./TeamImportModal";
import type { Group, Team } from "../../types";

// 「AI 创建」入口注入 composer 的提示词：引导 AI 走 create-team 技能，敲定编组后调 install_team 登记。
const CREATE_TEAM_PROMPT =
  "帮我创建一个团队。先问我要完成什么任务、需要哪些角色分工。然后按 create-team 技能设计编组：一名主理人（负责怎么安排、" +
  "把活分给谁，不直接干活）+ 若干成员（实际干活，每个写清角色设定 system_prompt 与产出格式），" +
  "并给几条开场引导语 quick_prompts。设计敲定后，你必须调用 install_team 工具来真正登记" +
  "（主理人/成员/开场引导语都在调用里现场定义）——只描述方案不算创建。登记会请求我确认；完成后团队出现在「团队」列表，可在会话激活。";

/** 团队页：列出/新建/启停/删除团队。团队 = lead + 成员（对现有 agent 的引用）。 */
/**
 * 团队页：管理协作团队（自建 / 导入 / 由能力包运送而来）。
 *
 * `embedded`：作为「扩展」页的团队 Tab 内嵌时隐藏自带 h1（与胶囊 Tab 标签重复）。
 * 注意：团队并入「扩展」页只是**导航形态**——运行时它仍是编排层（角色槽 + lead SOP +
 * roster）。团队自带的成员与技能是它**私有**的——激活该团队时才载入（T108）。
 */
export function TeamsPage({
  onOpenMarketSources: _onOpenMarketSources,
  embedded = false,
}: { onOpenMarketSources?: () => void; embedded?: boolean } = {}) {
  const messages = useMessages();
  const notifications = useNotifications();
  const { enterDraftWithContent, enterDraftWithTeam } = useSession();
  const [teams, setTeams] = useState<Team[]>([]);
  const [builderOpen, setBuilderOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [detailId, setDetailId] = useState<string | null>(null);
  const [mineQuery, setMineQuery] = useState("");
  const [groups, setGroups] = useState<Group[]>([]);
  // null=全部；"ungrouped"=未分组；其余=group id。
  const [selectedGroup, setSelectedGroup] = useState<string | null>(null);

  async function reload() {
    try {
      setTeams(await listTeams());
    } catch (err) {
      notifications.notify({ tone: "error", title: "加载失败", message: String(err) });
    }
  }

  async function reloadGroups() {
    try {
      setGroups(await listGroups("team"));
    } catch (err) {
      notifications.notify({ tone: "error", title: "加载分组失败", message: String(err) });
    }
  }

  useEffect(() => {
    void reload();
  }, []);



  async function handleCreateGroup(name: string) {
    try {
      await createGroup("team", name);
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
      message: `确定删除分组「${g.name}」吗？组内团队不会被删除，只会变成未分组。`,
      tone: "warning",
      confirmText: "删除",
    });
    if (!ok) return;
    try {
      await deleteGroup(g.id, "team");
      if (selectedGroup === g.id) setSelectedGroup(null);
      await Promise.all([reloadGroups(), reload()]);
    } catch (err) {
      notifications.notify({ tone: "error", title: "删除分组失败", message: String(err) });
    }
  }
  async function handleMoveTeam(team: Team, groupId: string | null) {
    try {
      await setTeamGroup(team.id, groupId);
      await reload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "移动失败", message: String(err) });
    }
  }

  const enabledCount = useMemo(() => teams.filter((team) => team.enabled).length, [teams]);
  const ungroupedCount = useMemo(() => teams.filter((t) => !t.groupId).length, [teams]);
  const countByGroup = useMemo(() => {
    const m: Record<string, number> = {};
    for (const t of teams) if (t.groupId) m[t.groupId] = (m[t.groupId] ?? 0) + 1;
    return m;
  }, [teams]);
  const filteredMineTeams = useMemo(() => {
    const q = mineQuery.trim().toLocaleLowerCase();
    return teams.filter((team) => {
      if (selectedGroup === "ungrouped" && team.groupId) return false;
      if (selectedGroup && selectedGroup !== "ungrouped" && team.groupId !== selectedGroup) return false;
      if (!q) return true;
      return [team.displayName, team.name, team.category, team.description]
        .filter(Boolean)
        .join(" ")
        .toLocaleLowerCase()
        .includes(q);
    });
  }, [mineQuery, teams, selectedGroup]);

  async function handleToggle(team: Team) {
    try {
      await toggleTeam(team.id, !team.enabled);
      await reload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "操作失败", message: String(err) });
    }
  }

  async function handleDelete(team: Team) {
    const ok = await messages.confirm({
      title: "删除团队",
      message: `确定删除团队「${team.displayName}」吗？团队专属的内容会一起删掉（作为成员加入的专家本身不受影响），删除后无法恢复。`,
      tone: "warning",
      confirmText: "删除",
    });
    if (!ok) return;
    try {
      await deleteTeam(team.id);
      await reload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "删除失败", message: String(err) });
    }
  }

  return (
    <div className="h-full overflow-auto px-6  py-3 text-sm">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-5 flex items-center justify-between gap-4">
          <div>
            {!embedded && <h1 className="text-xl font-semibold text-foreground">团队</h1>}
            <p className="mt-1 text-sm text-foreground">
              发现、导入和管理协作团队，由主理人调度成员一起完成任务。
            </p>
            {/* <span className="text-xs text-foreground-muted">
              已启用 {enabledCount} / {teams.length}
            </span> */}
           
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button tone="primary" onClick={() => enterDraftWithContent(CREATE_TEAM_PROMPT)}>
              <Sparkles className="h-4 w-4" aria-hidden="true" />
              AI 创建
            </Button>
            <Button tone="secondary" onClick={() => setImportOpen(true)}>
              <Plus className="h-4 w-4" aria-hidden="true" />
              安装
            </Button>
            {/* <Button tone="primary" onClick={() => setBuilderOpen(true)}>
              <Plus className="h-4 w-4" aria-hidden="true" />
              新建团队
            </Button> */}
          </div>
        </div>

       

        {teams.length === 0 ? (
          <EmptyState onCreate={() => enterDraftWithContent(CREATE_TEAM_PROMPT)} />
        ) : (
            <>
              <MineTeamSearch
                onQueryChange={setMineQuery}
                query={mineQuery}
              />
              <GroupFilterBar
                groups={groups}
                selected={selectedGroup}
                onSelect={setSelectedGroup}
                total={teams.length}
                ungroupedCount={ungroupedCount}
                countByGroup={countByGroup}
                onCreate={handleCreateGroup}
                onRename={handleRenameGroup}
                onDelete={handleDeleteGroup}
              />
              {filteredMineTeams.length === 0 ? (
                <div className="rounded-lg border border-dashed border-border px-4 py-12 text-center text-sm text-foreground-muted">
                  没有匹配的团队
                </div>
              ) : (
                <TeamList
                  groups={groups}
                  onDelete={handleDelete}
                  onMove={handleMoveTeam}
                  onOpen={setDetailId}
                  onToggle={handleToggle}
                  onUse={(teamId) => enterDraftWithTeam(teamId)}
                  teams={filteredMineTeams}
                />
              )}
          </>
        )}
      </div>

      <TeamBuilderModal
        open={builderOpen}
        onClose={() => setBuilderOpen(false)}
        onCreated={() => {
          setBuilderOpen(false);
          void reload();
        }}
      />
      <TeamImportModal
        open={importOpen}
        onClose={() => setImportOpen(false)}
        onImported={() => {
          setImportOpen(false);
          void reload();
        }}
      />
      <TeamDetailDrawer teamId={detailId} onClose={() => setDetailId(null)} />
    </div>
  );
}


function MineTeamSearch({
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
          placeholder="搜索我的团队名称、分类或描述"
          value={query}
          onChange={(event) => onQueryChange(event.target.value)}
        />
      </div>
    </div>
  );
}

/**
 * 团队列表。**行式连接列表**（与「插件」页同构）：一行一个，整块一个边框、行间分隔线。
 */
function TeamList({
  groups,
  onDelete,
  onMove,
  onOpen,
  onToggle,
  onUse,
  teams,
}: {
  groups: Group[];
  onDelete: (team: Team) => void | Promise<void>;
  onMove: (team: Team, groupId: string | null) => void | Promise<void>;
  onOpen: (teamId: string) => void;
  onToggle: (team: Team) => void | Promise<void>;
  onUse: (teamId: string) => void;
  teams: Team[];
}) {
  return (
    <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
      {teams.map((team, index) => (
        <TeamRow
          groups={groups}
          key={team.id}
          last={index === teams.length - 1}
          onDelete={onDelete}
          onMove={onMove}
          onOpen={onOpen}
          onToggle={onToggle}
          onUse={onUse}
          team={team}
        />
      ))}
    </ul>
  );
}

function TeamRow({
  groups,
  last,
  onDelete,
  onMove,
  onOpen,
  onToggle,
  onUse,
  team,
}: {
  groups: Group[];
  last: boolean;
  onDelete: (team: Team) => void | Promise<void>;
  onMove: (team: Team, groupId: string | null) => void | Promise<void>;
  onOpen: (teamId: string) => void;
  onToggle: (team: Team) => void | Promise<void>;
  onUse: (teamId: string) => void;
  team: Team;
}) {
  return (
    <li
      className={`group flex items-center gap-3.5 px-4 py-4 transition-colors hover:bg-primary/5 ${
        last ? "" : "border-b border-border-subtle"
      }`}
    >
      <TeamAvatar team={team} />

      {/* 标题行不整块套 <button>：分组下拉本身是按钮，嵌套 button 是非法 HTML。 */}
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => onOpen(team.id)}
            className="min-w-0 truncate text-left font-semibold text-foreground"
          >
            {team.displayName}
          </button>
          {team.memberCount > 0 && <Badge tone="neutral">{team.memberCount} 成员</Badge>}
          {team.category && <Badge tone="info">{team.category}</Badge>}
          <TeamGroupDropdown
            groups={groups}
            value={team.groupId}
            onChange={(gid) => void onMove(team, gid)}
          />
        </div>
        {team.description && (
          <button
            type="button"
            onClick={() => onOpen(team.id)}
            className="block w-full text-left"
          >
            <p className="mt-0.5 line-clamp-1 text-xs text-foreground-secondary">
              {team.description}
            </p>
          </button>
        )}
      </div>

      <div className="flex shrink-0 items-center gap-3">
        {/* 悬浮才出现：一屏几十行，常显的话按钮比内容还抢眼。 */}
        <div className="pointer-events-none flex items-center gap-1 opacity-0 transition group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100">
          <button
            type="button"
            onClick={() => onUse(team.id)}
            className="inline-flex shrink-0 items-center gap-1 rounded-md bg-primary px-2 py-1 text-xs font-medium text-primary-foreground transition hover:opacity-90"
          >
            <ArrowUpRight className="h-3.5 w-3.5" aria-hidden="true" />
            使用
          </button>
          {team.source === "user" && (
            <button
              type="button"
              onClick={() => void onDelete(team)}
              className="rounded-md px-2 py-1 text-xs text-foreground-muted transition hover:bg-accent hover:text-destructive"
            >
              删除
            </button>
          )}
        </div>

        <Switch checked={team.enabled} onChange={() => void onToggle(team)} />
      </div>
    </li>
  );
}

function TeamGroupDropdown({
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

function TeamAvatar({ team }: { team: Team }) {
  return (
    <div
      className={`grid h-10 w-10 shrink-0 place-items-center rounded-lg border border-border bg-background shadow-sm transition-colors ${
        team.enabled ? "text-primary" : "text-foreground-muted"
      }`}
    >
      <Users className="h-5 w-5" aria-hidden="true" />
    </div>
  );
}

function EmptyState({ onCreate }: { onCreate: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-border bg-surface/40 py-16 text-foreground-muted">
      <div className="grid h-12 w-12 place-items-center rounded-full bg-muted">
        <Users className="h-6 w-6" aria-hidden="true" />
      </div>
      <p className="text-sm">还没有团队</p>
      <Button tone="outline" onClick={onCreate}>
        <Sparkles className="h-4 w-4" aria-hidden="true" />
        AI 创建团队
      </Button>
    </div>
  );
}
