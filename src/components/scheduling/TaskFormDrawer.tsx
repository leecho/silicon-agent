import { useEffect, useRef, useState } from "react";
import { Button } from "../ui/Button";
import { Drawer, DrawerHeader } from "../ui/Drawer";
import { Select } from "../ui/Select";
import {
  getGlobalPermissionMode,
  getRecentWorkspaces,
  listActiveTeams,
  listAgents,
  listEnabledModels,
  listExperts,
  listProjects,
  listSkills,
  pickDirectory,
} from "../../api";
import { PermissionPicker } from "../../pages/session/composer/PermissionPicker";
import { ModelPicker } from "../../pages/session/composer/ModelPicker";
import { TeamPicker } from "../../pages/session/composer/TeamPicker";
import { WorkspacePicker } from "../../pages/session/composer/WorkspacePicker";
import { ComposerInput, type ComposerInputHandle } from "../session/ComposerInput";
import type {
  Agent,
  EnabledProviderModels,
  ExpertSummary,
  PermissionMode,
  Project,
  ScheduledTask,
  ScheduledTaskInput,
  ScheduleInput,
  Skill,
  Team,
} from "../../types";

// DOW name → weekday number (MON=1 … SUN=7), mirrors backend normalize_to_cron
const DOW_MAP: Record<string, number> = {
  MON: 1, TUE: 2, WED: 3, THU: 4, FRI: 5, SAT: 6, SUN: 7,
};

type ScheduleSeed =
  | { advanced: false; preset: "daily"; time: string }
  | { advanced: false; preset: "weekly"; time: string; weekdays: number[] }
  | { advanced: false; preset: "interval"; everyValue: number; everyUnit: "minutes" | "hours" }
  | { advanced: true; cronExpr: string };

/**
 * Reverse-parses a 6-field cron string (sec min hour dom month dow) back into
 * the UI form seed state. Mirrors the backend's `normalize_to_cron` normalization:
 *
 *   "0 M H * * *"            → daily at HH:MM
 *   "0 M H * * DOW[,…]"      → weekly on those weekdays at HH:MM (MON=1…SUN=7)
 *   "0 *\/N * * * *"          → interval every N minutes
 *   "0 0 *\/N * * *"          → interval every N hours
 *   anything else             → advanced (raw cron)
 */
export function parseScheduleSpec(spec: string): ScheduleSeed {
  const parts = spec.trim().split(/\s+/);
  if (parts.length !== 6) return { advanced: true, cronExpr: spec };

  const [sec, min, hour, dom, month, dow] = parts;

  // interval: every N minutes  →  0 */N * * * *
  const minuteInterval = min.match(/^\*\/(\d+)$/);
  if (
    sec === "0" &&
    minuteInterval &&
    hour === "*" &&
    dom === "*" &&
    month === "*" &&
    dow === "*"
  ) {
    return { advanced: false, preset: "interval", everyValue: Number(minuteInterval[1]), everyUnit: "minutes" };
  }

  // interval: every N hours  →  0 0 */N * * *
  const hourInterval = hour.match(/^\*\/(\d+)$/);
  if (
    sec === "0" &&
    min === "0" &&
    hourInterval &&
    dom === "*" &&
    month === "*" &&
    dow === "*"
  ) {
    return { advanced: false, preset: "interval", everyValue: Number(hourInterval[1]), everyUnit: "hours" };
  }

  // daily/weekly: sec=0, min/hour are plain integers, dom=*, month=*
  const minNum = /^\d+$/.test(min) ? Number(min) : NaN;
  const hourNum = /^\d+$/.test(hour) ? Number(hour) : NaN;

  if (sec === "0" && !isNaN(minNum) && !isNaN(hourNum) && dom === "*" && month === "*") {
    const hh = String(hourNum).padStart(2, "0");
    const mm = String(minNum).padStart(2, "0");
    const timeStr = `${hh}:${mm}`;

    if (dow === "*") {
      return { advanced: false, preset: "daily", time: timeStr };
    }

    // weekly: dow is a comma-separated list of DOW names
    const dowNames = dow.split(",");
    const nums = dowNames
      .map((d) => DOW_MAP[d.toUpperCase()])
      .filter((n): n is number => n !== undefined);

    if (nums.length === dowNames.length && nums.length > 0) {
      return { advanced: false, preset: "weekly", time: timeStr, weekdays: [...nums].sort((a, b) => a - b) };
    }
  }

  return { advanced: true, cronExpr: spec };
}

const WEEKDAYS = [
  { v: 1, label: "一" },
  { v: 2, label: "二" },
  { v: 3, label: "三" },
  { v: 4, label: "四" },
  { v: 5, label: "五" },
  { v: 6, label: "六" },
  { v: 7, label: "日" },
];

const SCHEDULE_OPTIONS = [
  { label: "每天", value: "daily" },
  { label: "每周", value: "weekly" },
  { label: "间隔", value: "interval" },
  { label: "高级 (cron)", value: "cron" },
];

const INTERVAL_UNIT_OPTIONS = [
  { label: "分钟", value: "minutes" },
  { label: "小时", value: "hours" },
];

const fieldClass =
  "w-full rounded-lg border border-border bg-surface px-3 py-2 text-sm text-foreground outline-none transition placeholder:text-foreground-secondary focus:border-ring";

const compactFieldClass =
  "h-9 rounded-lg border border-border bg-surface px-3 text-sm text-foreground outline-none transition focus:border-ring";

const labelClass = "mb-1.5 block  text-sm font-semibold text-foreground";

export function TaskFormDrawer({
  fixedAgentId,
  fixedProjectId,
  initial,
  onClose,
  onSubmit,
}: {
  fixedAgentId?: string | null;
  fixedProjectId?: string | null;
  initial?: ScheduledTask | null;
  onClose: () => void;
  onSubmit: (input: ScheduledTaskInput) => Promise<void>;
}) {
  const seed = initial?.scheduleSpec ? parseScheduleSpec(initial.scheduleSpec) : null;
  const composerRef = useRef<ComposerInputHandle | null>(null);
  const lockedEntity = Boolean(fixedProjectId || fixedAgentId);

  const [name, setName] = useState(initial?.name ?? "");
  const [prompt, setPrompt] = useState(initial?.prompt ?? "");
  const [preset, setPreset] = useState<"interval" | "daily" | "weekly">(
    (!seed || seed.advanced) ? "daily"
    : seed.preset
  );
  const [time, setTime] = useState(
    seed && !seed.advanced && seed.preset !== "interval" ? seed.time : "09:00"
  );
  const [weekdays, setWeekdays] = useState<number[]>(
    seed && !seed.advanced && seed.preset === "weekly" ? seed.weekdays : [1]
  );
  const [everyValue, setEveryValue] = useState(
    seed && !seed.advanced && seed.preset === "interval" ? seed.everyValue : 30
  );
  const [everyUnit, setEveryUnit] = useState<"minutes" | "hours">(
    seed && !seed.advanced && seed.preset === "interval" ? seed.everyUnit : "minutes"
  );
  const [advanced, setAdvanced] = useState(seed?.advanced ?? false);
  const [cronExpr, setCronExpr] = useState(
    seed?.advanced ? seed.cronExpr : (initial?.scheduleSpec ?? "0 0 9 * * *")
  );
  // 新建默认 full（全自主）；编辑沿用任务已存值（null=继承全局）。
  const [permissionMode, setPermissionMode] = useState<PermissionMode | null>(
    initial ? (initial.permissionMode ?? null) : "full",
  );
  const [modelId, setModelId] = useState<string | null>(initial?.modelId ?? null);
  const [workingDir, setWorkingDir] = useState<string | null>(
    fixedProjectId || fixedAgentId ? null : (initial?.workingDir ?? null),
  );
  const [projectId, setProjectId] = useState<string | null>(
    fixedProjectId ?? initial?.projectId ?? null,
  );
  const [agentId, setAgentId] = useState<string | null>(
    fixedAgentId ?? initial?.agentId ?? null,
  );
  const [roleKind, setRoleKind] = useState<"expert" | "team" | null>(
    initial?.roleKind ?? null,
  );
  const [roleId, setRoleId] = useState<string | null>(initial?.roleId ?? null);
  const [modelGroups, setModelGroups] = useState<EnabledProviderModels[]>([]);
  const [globalPermMode, setGlobalPermMode] = useState<PermissionMode>("manual");
  const [recentWorkspaces, setRecentWorkspaces] = useState<string[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [teams, setTeams] = useState<Team[]>([]);
  const [roleExperts, setRoleExperts] = useState<ExpertSummary[]>([]);
  const [skills, setSkills] = useState<Skill[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void listEnabledModels().then(setModelGroups).catch(() => setModelGroups([]));
    void getGlobalPermissionMode().then(setGlobalPermMode).catch(() => {});
    void getRecentWorkspaces().then(setRecentWorkspaces).catch(() => setRecentWorkspaces([]));
    void listProjects().then(setProjects).catch(() => setProjects([]));
    void listAgents().then(setAgents).catch(() => setAgents([]));
    void listActiveTeams().then(setTeams).catch(() => setTeams([]));
    void listExperts().then(setRoleExperts).catch(() => setRoleExperts([]));
    void listSkills()
      .then((items) => setSkills(items.filter((skill) => skill.enabled && skill.userInvocable)))
      .catch(() => setSkills([]));
  }, []);

  async function handlePickWorkspace() {
    const picked = await pickDirectory();
    if (!picked) return;
    setProjectId(null);
    setAgentId(null);
    setWorkingDir(picked);
  }

  function pickProject(nextProjectId: string) {
    setProjectId(nextProjectId);
    setAgentId(null);
    setWorkingDir(null);
    setRoleKind(null);
    setRoleId(null);
  }

  function pickAgent(nextAgentId: string) {
    setAgentId(nextAgentId);
    setProjectId(null);
    setWorkingDir(null);
    setRoleKind(null);
    setRoleId(null);
  }

  function pickRecentWorkspace(path: string) {
    setProjectId(null);
    setAgentId(null);
    setWorkingDir(path);
  }

  function pickRole(kind: string, id: string) {
    if (kind === "expert" || kind === "team") {
      setRoleKind(kind);
      setRoleId(id);
      return;
    }
    setRoleKind(null);
    setRoleId(null);
  }

  // 生效权限模式（null 时取全局默认）。非 full 时给风险提示。
  const effectiveMode: PermissionMode = permissionMode ?? globalPermMode;

  function buildSchedule(): ScheduleInput {
    if (advanced) return { kind: "cron", expr: cronExpr };
    if (preset === "interval") {
      return {
        kind: "preset",
        preset,
        time: "",
        weekdays: [],
        every: { value: everyValue, unit: everyUnit },
      };
    }
    return { kind: "preset", preset, time, weekdays: preset === "weekly" ? weekdays : [] };
  }

  function scheduleDisplay(): string {
    if (advanced) return `cron: ${cronExpr}`;
    if (preset === "interval") return `每 ${everyValue} ${everyUnit === "minutes" ? "分钟" : "小时"}`;
    if (preset === "daily") return `每天 ${time}`;
    return `每周 ${weekdays.map((w) => WEEKDAYS[w - 1].label).join("")} ${time}`;
  }

  async function handleSave(promptOverride?: string) {
    const promptText = (promptOverride ?? composerRef.current?.getText() ?? prompt).trim();
    setError(null);
    setSaving(true);
    try {
      await onSubmit({
        name,
        prompt: promptText,
        schedule: buildSchedule(),
        scheduleDisplay: scheduleDisplay(),
        permissionMode,
        modelId,
        workingDir: projectId || agentId ? null : workingDir,
        projectId,
        agentId,
        roleKind: projectId || agentId ? null : roleKind,
        roleId: projectId || agentId ? null : roleId,
      });
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <Drawer
      open
      onClose={onClose}
      title={initial ? "编辑任务" : "新建定时任务"}
      className="grid-rows-[minmax(0,1fr)]"
      widthClassName="w-[min(820px,94vw)]"
    >
      <form
        className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)_auto]"
        onSubmit={(event) => {
          event.preventDefault();
          void handleSave();
        }}
      >
        <DrawerHeader onClose={onClose}>
          <h2 className="text-base font-semibold text-foreground">
            {initial ? "编辑任务" : "新建定时任务"}
          </h2>
        </DrawerHeader>

        <div className="min-h-0 overflow-y-auto px-5 py-4">
          <div className="space-y-5">
            <div>
              <label className={labelClass}>任务名称</label>
              <input
                className={fieldClass}
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="给任务取个名字"
              />
            </div>

            <div>
              <div className="mb-2">
                <label className={labelClass}>计划时间</label>
                <p className="text-xs leading-5 text-foreground-muted">
                  当前计划：{scheduleDisplay()}
                </p>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <Select
                  className="w-[128px]"
                  value={advanced ? "cron" : preset}
                  onChange={(nextValue) => {
                    if (nextValue === "cron") {
                      setAdvanced(true);
                    } else {
                      setAdvanced(false);
                      setPreset(nextValue as "interval" | "daily" | "weekly");
                    }
                  }}
                  options={SCHEDULE_OPTIONS}
                />
                {!advanced && preset !== "interval" && (
                  <input
                    type="time"
                    className={`${compactFieldClass} w-[128px]`}
                    value={time}
                    onChange={(e) => setTime(e.target.value)}
                  />
                )}
                {!advanced && preset === "interval" && (
                  <>
                    <input
                      type="number"
                      min={1}
                      className={`${compactFieldClass} w-24`}
                      value={everyValue}
                      onChange={(e) => setEveryValue(Number(e.target.value))}
                    />
                    <Select
                      className="w-[112px]"
                      value={everyUnit}
                      onChange={(nextValue) => setEveryUnit(nextValue as "minutes" | "hours")}
                      options={INTERVAL_UNIT_OPTIONS}
                    />
                  </>
                )}
                {advanced && (
                  <input
                    className={`${compactFieldClass} min-w-[220px] flex-1 font-mono`}
                    value={cronExpr}
                    onChange={(e) => setCronExpr(e.target.value)}
                    placeholder="0 0 9 * * *"
                  />
                )}
              </div>
              {!advanced && preset === "weekly" && (
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {WEEKDAYS.map((d) => (
                    <button
                      key={d.v}
                      type="button"
                      className={`h-8 w-8 rounded-lg text-sm transition ${
                        weekdays.includes(d.v)
                          ? "bg-primary text-primary-foreground"
                          : "border border-border bg-surface text-foreground-secondary hover:bg-accent hover:text-foreground"
                      }`}
                      onClick={() =>
                        setWeekdays((ws) =>
                          ws.includes(d.v) ? ws.filter((x) => x !== d.v) : [...ws, d.v],
                        )
                      }
                    >
                      {d.label}
                    </button>
                  ))}
                </div>
              )}
            </div>
            <div>
              <label className={labelClass}>让 Agent 帮你做什么</label>
              <div className="rounded-xl border border-border bg-surface ">
                <ComposerInput
                  key={initial?.id ?? "new-task"}
                  ref={composerRef}
                  skills={skills}
                  workspaceFiles={[]}
                  initialContent={initial?.prompt ?? ""}
                  maxHeightClassName="max-h-[320px]"
                  minHeightClassName="min-h-[180px]"
                  placeholder="描述你希望 Agent 定时执行的任务..."
                  onSubmit={(text) => void handleSave(text)}
                  onContentChange={() => setPrompt(composerRef.current?.getText() ?? "")}
                />
              </div>
              <div className="flex flex-wrap items-center gap-2 px-2 py-2">
                <WorkspacePicker
                  projects={projects}
                  selectedProjectId={projectId}
                  agents={agents}
                  selectedAgentId={agentId}
                  workspaceName={workingDir ? workingDir.split(/[/\\]/).pop() : undefined}
                  workspacePath={workingDir ?? undefined}
                  onPickProject={lockedEntity ? undefined : pickProject}
                  onPickAgent={lockedEntity ? undefined : pickAgent}
                  onPickWorkspace={lockedEntity ? undefined : () => void handlePickWorkspace()}
                  recentWorkspaces={lockedEntity ? [] : recentWorkspaces}
                  onPickRecent={lockedEntity ? undefined : pickRecentWorkspace}
                  onClear={
                    lockedEntity
                      ? undefined
                      : () => {
                          setWorkingDir(null);
                          setProjectId(null);
                          setAgentId(null);
                        }
                  }
                  locked={lockedEntity}
                />
                {!projectId && !agentId && (
                  <TeamPicker
                    value={{ kind: roleKind ?? "", id: roleId ?? "" }}
                    teams={teams}
                    agents={roleExperts}
                    onPick={pickRole}
                  />
                )}
                  <PermissionPicker
                    value={permissionMode}
                    globalDefault={globalPermMode}
                    onChange={(m) => setPermissionMode(m)}
                  />
                  <ModelPicker
                    modelGroups={modelGroups}
                    selectedModelId={modelId}
                    onPick={(id) => setModelId(id)}
                  />
              </div>
              {effectiveMode !== "full" && (
                <div className="mt-3 rounded-lg border border-warning/90 bg-warning/80 text-[#ffffff] px-3 py-2 text-xs leading-5 ">
                  非 <b>完全授权</b> 模式下，任务遇到需要确认的工具或 Agent 提问时会暂停并标记「需关注」，
                  无人时可能无法自动完成 —— 需你下次打开应用手动处理。定时任务建议用 <b>完全授权</b> 全自主运行。
                </div>
              )}
            </div>

            {error && (
              <div className="rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                {error}
              </div>
            )}
          </div>
        </div>

        <div className="flex shrink-0 justify-end gap-2 border-t border-border px-5 py-4">
          <Button type="button" tone="outline" onClick={onClose}>
            取消
          </Button>
          <Button
            tone="primary"
            type="submit"
            disabled={saving || !name.trim() || !(composerRef.current?.getText() ?? prompt).trim()}
          >
            {saving ? "保存中…" : "保存"}
          </Button>
        </div>
      </form>
    </Drawer>
  );
}
