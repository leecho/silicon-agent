import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ArrowUpRight, ChevronDown, Folder, Plus, Sparkles, Wrench } from "lucide-react";
import {
  createGroup,
  deleteGroup,
  listGroups,
  listSkills,
  renameGroup,
  setSkillGroup,
  toggleSkill,
  uninstallSkill,
} from "../../api";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import { GroupFilterBar } from "../../components/groups/GroupFilterBar";
import { DropdownMenu, type DropdownMenuAnchor, type DropdownMenuEntry } from "../../components/ui";
import { useSession } from "../../components/session/SessionProvider";
import { useMessages } from "../../components/ui/MessageProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { skillIcon } from "../../lib/skillPresentation";
import type { Group, Skill } from "../../types";
import { SkillDetailDrawer } from "./SkillDetailDrawer";
import { SkillInstallModal } from "./SkillInstallModal";
import { OwnerGroupTitle } from "../extensions/OwnerGroupTitle";
import { Switch } from "../../components/ui/Switch";

// 「使用 AI 创建技能」入口注入 composer 的提示词。按本项目实际流程编写：
// 技能在会话工作区内创作，再由 install_skill 工具登记（需用户确认）——本应用无虚拟机运行时，
// 模型也无法直接写受管 skills 目录，故不做"环境检测/写固定路径"。
const CREATE_SKILL_PROMPT =
  "帮我使用 create-skill 创建一个技能。在当前工作目录中创作好技能目录后，" +
  "调用 install_skill 工具完成登记（会请求我确认），登记后即可在技能列表中使用。" +
  "请先问我这个技能应该做什么。";

/**
 * `embedded`：作为「扩展」页的技能 Tab 内嵌时——隐藏自带 h1（与胶囊 Tab 标签重复）
 * 与内部「技能广场」子 Tab（广场已统一到「扩展 → 市场」Tab）。T106 §5.2。
 */
export function SkillsPage({
  embedded = false,
}: { embedded?: boolean } = {}) {
  const messages = useMessages();
  const notifications = useNotifications();
  const { enterDraftWithContent } = useSession();
  const [skills, setSkills] = useState<Skill[]>([]);
  const [detailId, setDetailId] = useState<string | null>(null);
  const [installOpen, setInstallOpen] = useState(false);
  const [groups, setGroups] = useState<Group[]>([]);
  // null=全部；"ungrouped"=未分组；其余=group id。
  const [selectedGroup, setSelectedGroup] = useState<string | null>(null);

  async function reload() {
    try {
      setSkills(await listSkills());
    } catch (err) {
      notifications.notify({ tone: "error", title: "加载失败", message: String(err) });
    }
  }

  async function reloadGroups() {
    try {
      setGroups(await listGroups("skill"));
    } catch (err) {
      notifications.notify({ tone: "error", title: "加载分组失败", message: String(err) });
    }
  }

  useEffect(() => {
    void reload();
  }, []);

  useEffect(() => {
    void reloadGroups();
  }, []);

  async function handleCreateGroup(name: string) {
    try {
      await createGroup("skill", name);
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
      message: `确定删除分组「${g.name}」吗？组内技能不会被删除，只会变成未分组。`,
      tone: "warning",
      confirmText: "删除",
    });
    if (!ok) return;
    try {
      await deleteGroup(g.id, "skill");
      if (selectedGroup === g.id) setSelectedGroup(null);
      await Promise.all([reloadGroups(), reload()]);
    } catch (err) {
      notifications.notify({ tone: "error", title: "删除分组失败", message: String(err) });
    }
  }
  async function handleMoveSkill(skill: Skill, groupId: string | null) {
    try {
      await setSkillGroup(skill.id, groupId);
      await reload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "移动失败", message: String(err) });
    }
  }

  const enabledCount = useMemo(() => skills.filter((s) => s.enabled).length, [skills]);
  const ungroupedCount = useMemo(() => skills.filter((s) => !s.groupId).length, [skills]);
  const countByGroup = useMemo(() => {
    const m: Record<string, number> = {};
    for (const s of skills) if (s.groupId) m[s.groupId] = (m[s.groupId] ?? 0) + 1;
    return m;
  }, [skills]);
  const filteredMineSkills = useMemo(() => {
    return skills.filter((skill) => {
      if (selectedGroup === "ungrouped" && skill.groupId) return false;
      if (selectedGroup && selectedGroup !== "ungrouped" && skill.groupId !== selectedGroup) return false;
      return true;
    });
  }, [skills, selectedGroup]);
  // owner 分组（T106 §5.3）：「我的」= 用户自建/装的；「来自插件」= 随插件带来的。
  const ownSkills = useMemo(
    () => filteredMineSkills.filter((s) => !s.pluginId),
    [filteredMineSkills],
  );
  const pluginSkills = useMemo(
    () => filteredMineSkills.filter((s) => s.pluginId),
    [filteredMineSkills],
  );

  async function handleToggle(skill: Skill) {
    try {
      await toggleSkill(skill.id, !skill.enabled);
      await reload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "操作失败", message: String(err) });
    }
  }

  async function handleUninstall(skill: Skill) {
    const ok = await messages.confirm({
      title: "卸载技能",
      message: `确定卸载技能「${skill.name}」吗？将删除其磁盘目录，操作不可撤销。`,
      tone: "warning",
      confirmText: "卸载",
    });
    if (!ok) return;
    try {
      await uninstallSkill(skill.id);
      await reload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "卸载失败", message: String(err) });
    }
  }

  return (
    <div className="h-full overflow-auto px-6 py-3 text-sm">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-5 flex items-center justify-between gap-4">
          <div>
            {!embedded && <h1 className="text-xl font-semibold text-foreground">技能</h1>}
            <p className="mt-1 text-sm text-foreground">
              发现、安装和管理技能，启用后模型可按需加载详情。
            </p>
            {/* {skills.length > 0 && (
              <span className="text-xs text-foreground-muted">
                已启用 {enabledCount} / {skills.length}
              </span>
            )} */}
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button tone="primary" onClick={() => enterDraftWithContent(CREATE_SKILL_PROMPT)}>
              <Sparkles className="h-4 w-4" aria-hidden="true" />
              AI 创建
            </Button>
            <Button tone="secondary" onClick={() => setInstallOpen(true)}>
              <Plus className="h-4 w-4" aria-hidden="true" />
              安装
            </Button>
          </div>
        </div>

        {skills.length === 0 ? (
          <EmptyState onInstall={() => setInstallOpen(true)} />
        ) : (
            <>
              <GroupFilterBar
                groups={groups}
                selected={selectedGroup}
                onSelect={setSelectedGroup}
                total={skills.length}
                ungroupedCount={ungroupedCount}
                countByGroup={countByGroup}
                onCreate={handleCreateGroup}
                onRename={handleRenameGroup}
                onDelete={handleDeleteGroup}
              />
              {filteredMineSkills.length === 0 ? (
                <div className="rounded-lg border border-dashed border-border px-4 py-12 text-center text-sm text-foreground-muted">
                  没有匹配的技能
                </div>
              ) : (
                <div className="flex flex-col gap-6">
                  {ownSkills.length > 0 && (
                    <section>
                      <OwnerGroupTitle title="我的" count={ownSkills.length} />
                      <SkillList
                        groups={groups}
                        onMove={handleMoveSkill}
                        onOpen={setDetailId}
                        onToggle={handleToggle}
                        onUninstall={handleUninstall}
                        onUse={(skillName) => enterDraftWithContent(`⟦技能：${skillName}⟧ `)}
                        skills={ownSkills}
                      />
                    </section>
                  )}
                  {pluginSkills.length > 0 && (
                    <section>
                      <OwnerGroupTitle
                        title="来自插件"
                        count={pluginSkills.length}
                        hint="随插件安装，卸载插件即一并移除"
                      />
                      <SkillList
                        groups={groups}
                        onOpen={setDetailId}
                        onUse={(skillName) => enterDraftWithContent(`⟦技能：${skillName}⟧ `)}
                        readOnly
                        skills={pluginSkills}
                      />
                    </section>
                  )}
                </div>
              )}
          </>
        )}
      </div>

      <SkillInstallModal
        open={installOpen}
        onClose={() => setInstallOpen(false)}
        onInstalled={() => {
          setInstallOpen(false);
          void reload();
        }}
      />
      <SkillDetailDrawer skillId={detailId} onClose={() => setDetailId(null)} />
    </div>
  );
}


/**
 * 技能列表。**行式连接列表**（与「插件」页同构）：一行一个，整块一个边框、行间分隔线。
 *
 * 不用卡片网格：技能是「一条条能力」，不是「一件件商品」——
 * 行式列表能一屏看更多、名字左对齐好扫，也和插件页视觉一致。
 */
function SkillList({
  groups = [],
  onMove,
  onOpen,
  onToggle,
  onUninstall,
  onUse,
  readOnly = false,
  skills,
}: {
  groups?: Group[];
  /** 只读：插件带来的技能随插件启停，不可单独开关（T53 / T106 §5.2）。 */
  readOnly?: boolean;
  onMove?: (skill: Skill, groupId: string | null) => void | Promise<void>;
  onOpen: (skillId: string) => void;
  onToggle?: (skill: Skill) => void | Promise<void>;
  onUninstall?: (skill: Skill) => void | Promise<void>;
  onUse: (skillName: string) => void;
  skills: Skill[];
}) {
  return (
    <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
      {skills.map((skill, index) => (
        <SkillRow
          groups={groups}
          key={skill.id}
          last={index === skills.length - 1}
          onMove={onMove}
          onOpen={onOpen}
          onToggle={onToggle}
          onUninstall={onUninstall}
          onUse={onUse}
          readOnly={readOnly}
          skill={skill}
        />
      ))}
    </ul>
  );
}

function SkillRow({
  groups,
  last,
  onMove,
  onOpen,
  onToggle,
  onUninstall,
  onUse,
  readOnly = false,
  skill,
}: {
  groups: Group[];
  last: boolean;
  readOnly?: boolean;
  onMove?: (skill: Skill, groupId: string | null) => void | Promise<void>;
  onOpen: (skillId: string) => void;
  onToggle?: (skill: Skill) => void | Promise<void>;
  onUninstall?: (skill: Skill) => void | Promise<void>;
  onUse: (skillName: string) => void;
  skill: Skill;
}) {
  const Icon = skillIcon(skill);

  return (
    <li
      className={`group flex items-center gap-3.5 px-4 py-4 transition-colors hover:bg-primary/5 ${
        last ? "" : "border-b border-border-subtle"
      }`}
    >
      <div
        className={`grid h-10 w-10 shrink-0 place-items-center rounded-lg border border-border bg-background shadow-sm transition-colors ${
          skill.enabled ? "text-primary" : "text-foreground-muted"
        }`}
      >
        <Icon className="h-5 w-5" aria-hidden="true" />
      </div>

      {/* 标题行不能整块套一个 <button>：分组下拉本身是按钮，嵌套 button 是非法 HTML
          （浏览器会把它拆出去，点击行为随之失灵）。故名称与描述各自是可点区域。 */}
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => onOpen(skill.id)}
            className="min-w-0 truncate text-left font-semibold text-foreground"
          >
            {/* plugin 提供的技能显示限定名（`plugin:name`，T108 §6）：装两个都带同名技能的
                plugin 时，裸名无从区分；模型面用的也是这个名字，两边得对得上。 */}
            {skill.qualifiedName ?? skill.name}
          </button>
          {skill.source === "builtin" && <Badge tone="neutral">内置</Badge>}
          {skill.userInvocable && <Badge tone="info">可调用</Badge>}
          {!readOnly && onMove && (
            <SkillGroupDropdown
              groups={groups}
              value={skill.groupId}
              onChange={(gid) => void onMove(skill, gid)}
            />
          )}
        </div>
        {skill.description && (
          <button
            type="button"
            onClick={() => onOpen(skill.id)}
            className="block w-full text-left"
          >
            <p className="mt-0.5 line-clamp-1 text-xs text-foreground-secondary">
              {skill.description}
            </p>
          </button>
        )}
      </div>

      <div className="flex shrink-0 items-center gap-3">
        {/* 悬浮才出现：一屏几十行，常显的话按钮比内容还抢眼。 */}
        <div className="pointer-events-none flex items-center gap-1 opacity-0 transition group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100">
          {skill.userInvocable && (
            <button
              type="button"
              onClick={() => onUse(skill.name)}
              className="inline-flex shrink-0 items-center gap-1 rounded-md bg-primary px-2 py-1 text-xs font-medium text-primary-foreground transition hover:opacity-90"
            >
              <ArrowUpRight className="h-3.5 w-3.5" aria-hidden="true" />
              使用
            </button>
          )}
          {!readOnly && skill.source === "user" && (
            <button
              type="button"
              onClick={() => void onUninstall?.(skill)}
              className="rounded-md px-2 py-1 text-xs text-foreground-muted transition hover:bg-accent hover:text-destructive"
            >
              卸载
            </button>
          )}
        </div>

        {readOnly ? (
          // 插件带来的技能随插件启停 —— 给的是状态，不是开关，否则用户点了却没反应。
          <span className="text-xs text-foreground-muted">
            {skill.enabled ? "已启用" : "已停用"} · 随插件
          </span>
        ) : (
          <Switch checked={skill.enabled} onChange={() => void onToggle?.(skill)} />
        )}
      </div>
    </li>
  );
}


function SkillGroupDropdown({
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
    setAnchorRect({ bottom: rect.bottom, left: rect.left, right: rect.right, top: rect.top });
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

function EmptyState({ onInstall }: { onInstall: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-border bg-surface/40 py-16 text-foreground-muted">
      <div className="grid h-12 w-12 place-items-center rounded-full bg-muted">
        <Wrench className="h-6 w-6" aria-hidden="true" />
      </div>
      <p className="text-sm">还没有技能</p>
      <Button tone="outline" onClick={onInstall}>
        <Plus className="h-4 w-4" aria-hidden="true" />
        安装技能
      </Button>
    </div>
  );
}
