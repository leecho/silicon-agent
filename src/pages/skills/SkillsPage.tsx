import { useEffect, useMemo, useState, type ReactNode } from "react";
import { ArrowUpRight, Plus, Search, Sparkles, Wrench } from "lucide-react";
import {
  listSkills,
  toggleSkill,
  uninstallSkill,
} from "../../api";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import { useSession } from "../../components/session/SessionProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { skillIcon } from "../../lib/skillPresentation";
import type { Skill } from "../../types";
import { SkillDetailDrawer } from "./SkillDetailDrawer";
import { SkillInstallModal } from "./SkillInstallModal";
import { Switch } from "../../components/ui/Switch";
import { useMessages } from "../../components/ui/MessageProvider";

type SkillPageTab = "plaza" | "mine";

// 「使用 AI 创建技能」入口注入 composer 的提示词。按本项目实际流程编写：
// 技能在会话工作区内创作，再由 install_skill 工具登记（需用户确认）——本应用无虚拟机运行时，
// 模型也无法直接写受管 skills 目录，故不做"环境检测/写固定路径"。
const CREATE_SKILL_PROMPT =
  "帮我使用 create-skill 创建一个技能。在当前工作目录中创作好技能目录后，" +
  "调用 install_skill 工具完成登记（会请求我确认），登记后即可在技能列表中使用。" +
  "请先问我这个技能应该做什么。";

export function SkillsPage() {
  const messages = useMessages();
  const notifications = useNotifications();
  const { enterDraftWithContent } = useSession();
  const [skills, setSkills] = useState<Skill[]>([]);
  const [tab, setTab] = useState<SkillPageTab>("plaza");
  const [detailId, setDetailId] = useState<string | null>(null);
  const [installOpen, setInstallOpen] = useState(false);

  async function reload() {
    try {
      setSkills(await listSkills());
    } catch (err) {
      notifications.notify({ tone: "error", title: "加载失败", message: String(err) });
    }
  }

  useEffect(() => {
    void reload();
  }, []);

  const builtin = useMemo(() => skills.filter((s) => s.source === "builtin"), [skills]);
  const enabledCount = useMemo(() => skills.filter((s) => s.enabled).length, [skills]);

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
    <div className="h-full overflow-auto p-6 text-sm">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-5 mt-4 flex items-start justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold text-foreground">技能</h1>
            <p className="mt-1 text-xs text-foreground-muted">
              发现、安装和管理技能，启用后模型可按需加载详情。
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button tone="secondary" onClick={() => enterDraftWithContent(CREATE_SKILL_PROMPT)}>
              <Sparkles className="h-4 w-4" aria-hidden="true" />
              AI 创建
            </Button>
            <Button tone="primary" onClick={() => setInstallOpen(true)}>
              <Plus className="h-4 w-4" aria-hidden="true" />
              安装
            </Button>
          </div>
        </div>

        <div className="mb-5 flex items-end justify-between gap-3 border-b border-border-subtle">
          <div className="flex items-end gap-2">
            <TabButton active={tab === "plaza"} onClick={() => setTab("plaza")}>
              技能广场
            </TabButton>
            <TabButton active={tab === "mine"} onClick={() => setTab("mine")}>
              我的技能
              <Badge tone={tab === "mine" ? "info" : "neutral"}>{skills.length}</Badge>
            </TabButton>
          </div>
          {tab === "mine" && skills.length > 0 && (
            <span className="pb-2.5 text-xs text-foreground-muted">
              已启用 {enabledCount} / {skills.length}
            </span>
          )}
        </div>

        {tab === "plaza" && (
          <SkillPlaza
            skills={builtin}
            onOpen={setDetailId}
            onUse={(skillName) => enterDraftWithContent(`⟦技能：${skillName}⟧ `)}
          />
        )}

        {tab === "mine" && (
          skills.length === 0 ? (
            <EmptyState onInstall={() => setInstallOpen(true)} />
          ) : (
            <SkillGrid
              onOpen={setDetailId}
              onToggle={handleToggle}
              onUninstall={handleUninstall}
              onUse={(skillName) => enterDraftWithContent(`⟦技能：${skillName}⟧ `)}
              skills={skills}
            />
          )
        )}
      </div>

      <SkillInstallModal
        open={installOpen}
        onClose={() => setInstallOpen(false)}
        onInstalled={() => {
          setInstallOpen(false);
          setTab("mine");
          void reload();
        }}
      />
      <SkillDetailDrawer skillId={detailId} onClose={() => setDetailId(null)} />
    </div>
  );
}

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`-mb-px flex items-center gap-2 border-b-2 px-3 py-2.5 text-sm transition-colors ${
        active
          ? "border-primary font-medium text-foreground"
          : "border-transparent text-foreground-secondary hover:text-foreground"
      }`}
    >
      {children}
    </button>
  );
}

function SkillPlaza({
  onOpen,
  onUse,
  skills,
}: {
  onOpen: (skillId: string) => void;
  onUse: (skillName: string) => void;
  skills: Skill[];
}) {
  const [query, setQuery] = useState("");
  const filtered = useMemo(() => {
    const q = query.trim().toLocaleLowerCase();
    if (!q) return skills;
    return skills.filter((skill) =>
      [skill.name, skill.description, skill.argumentHint]
        .filter(Boolean)
        .join(" ")
        .toLocaleLowerCase()
        .includes(q),
    );
  }, [query, skills]);

  return (
    <>
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <div className="relative min-w-[220px] flex-1">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-foreground-muted" aria-hidden="true" />
          <input
            className="w-full rounded-md border border-border bg-background py-2 pl-8 pr-3 text-sm text-foreground outline-none focus:border-primary"
            placeholder="搜索技能名称或描述"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>
      </div>
      {filtered.length === 0 ? (
        <div className="rounded-lg border border-dashed border-border px-4 py-12 text-center text-sm text-foreground-muted">
          没有匹配的技能
        </div>
      ) : (
        <SkillGrid
          mode="plaza"
          onOpen={onOpen}
          onUse={onUse}
          skills={filtered}
        />
      )}
    </>
  );
}

function SkillGrid({
  mode = "mine",
  onOpen,
  onToggle,
  onUninstall,
  onUse,
  skills,
}: {
  mode?: "mine" | "plaza";
  onOpen: (skillId: string) => void;
  onToggle?: (skill: Skill) => void | Promise<void>;
  onUninstall?: (skill: Skill) => void | Promise<void>;
  onUse: (skillName: string) => void;
  skills: Skill[];
}) {
  const plazaGridClass = "grid grid-cols-1 items-start gap-3 sm:grid-cols-2 lg:grid-cols-3";
  const mineGridClass = "grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3";

  return (
    <div className={mode === "plaza" ? plazaGridClass : mineGridClass}>
      {skills.map((skill) => (
        <SkillCard
          key={skill.id}
          mode={mode}
          onOpen={onOpen}
          onToggle={onToggle}
          onUninstall={onUninstall}
          onUse={onUse}
          skill={skill}
        />
      ))}
    </div>
  );
}

function SkillCard({
  mode,
  onOpen,
  onToggle,
  onUninstall,
  onUse,
  skill,
}: {
  mode: "mine" | "plaza";
  onOpen: (skillId: string) => void;
  onToggle?: (skill: Skill) => void | Promise<void>;
  onUninstall?: (skill: Skill) => void | Promise<void>;
  onUse: (skillName: string) => void;
  skill: Skill;
}) {
  const Icon = skillIcon(skill);
  const sourceLabel = skill.source === "builtin" ? "内置" : "用户安装";
  const plazaCardClass =
    "group flex flex-col rounded-xl border border-border-subtle bg-surface p-3 transition hover:border-border";
  const mineCardClass =
    "group flex min-h-[168px] flex-col rounded-xl border border-border-subtle bg-surface p-4 transition hover:border-border";
  const plazaIconClass =
    "grid h-9 w-9 shrink-0 place-items-center rounded-lg border border-border bg-background shadow-sm transition-colors";
  const mineIconClass =
    "grid h-11 w-11 shrink-0 place-items-center rounded-lg border border-border bg-background shadow-sm transition-colors";
  const plazaDescriptionClass = "mt-2 line-clamp-2 text-xs leading-5 text-foreground-secondary";
  const mineDescriptionClass = "mt-2 line-clamp-2 text-xs leading-5 text-foreground-secondary";

  return (
    <div className={mode === "plaza" ? plazaCardClass : mineCardClass}>
      <div className={mode === "plaza" ? "flex items-start gap-2.5" : "flex items-start gap-3"}>
        <div
          className={`${mode === "plaza" ? plazaIconClass : mineIconClass} ${
            skill.enabled ? "text-primary" : "text-foreground-muted"
          }`}
        >
          <Icon className={mode === "plaza" ? "h-4 w-4" : "h-5 w-5"} aria-hidden="true" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => onOpen(skill.id)}
              className="min-w-0 flex-1 text-left"
            >
              <p className="truncate font-semibold text-foreground">{skill.name}</p>
            </button>
            {/* plaza action */}
            {mode === "plaza" && skill.userInvocable && (
              <button
                type="button"
                onClick={() => onUse(skill.name)}
                className="pointer-events-none inline-flex shrink-0 items-center gap-1 rounded-md bg-primary px-2 py-1 text-xs font-medium text-primary-foreground opacity-0 transition hover:opacity-90 group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100"
              >
                <ArrowUpRight className="h-3.5 w-3.5" aria-hidden="true" />
                使用
              </button>
            )}
          </div>
          <p className="truncate text-xs text-foreground-muted">
            {sourceLabel}
            {skill.userInvocable ? " · 可调用" : ""}
          </p>
        </div>
      </div>

      <button
        type="button"
        onClick={() => onOpen(skill.id)}
        className={mode === "plaza" ? "text-left" : "min-h-[42px] text-left"}
      >
        {skill.description && (
          <p className={mode === "plaza" ? plazaDescriptionClass : mineDescriptionClass}>
            {skill.description}
          </p>
        )}
      </button>

      {mode === "mine" && (
        <div className="mt-auto flex items-center justify-between gap-3 pt-3">
          <div className="flex items-center gap-2">
            <Switch checked={skill.enabled} onChange={() => void onToggle?.(skill)} />
            <span className="text-xs text-foreground-muted">
              {skill.enabled ? "已启用" : "已停用"}
            </span>
          </div>
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
            {skill.source === "user" && (
              <button
                type="button"
                onClick={() => void onUninstall?.(skill)}
                className="inline-flex shrink-0 rounded-md px-2 py-1 text-xs text-foreground-muted transition hover:bg-accent hover:text-destructive"
              >
                卸载
              </button>
            )}
          </div>
        </div>
      )}
    </div>
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
