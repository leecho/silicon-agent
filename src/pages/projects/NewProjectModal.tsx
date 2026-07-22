import { useEffect, useMemo, useState } from "react";
import { Bot, FileText, FolderOpen, Loader2, Pencil, Trash2, UserPlus, Users } from "lucide-react";

import {
  addProjectMember,
  createProject,
  getExpertDetail,
  getTeamDetail,
  importTeamMember,
  listStandaloneExperts,
  listTeams,
  pickDirectory,
} from "../../api";
import { Button } from "../../components/ui/Button";
import { BubbleConfirm } from "../../components/ui/BubbleConfirm";
import { Drawer, DrawerHeader } from "../../components/ui/Drawer";
import { useMessages } from "../../components/ui/MessageProvider";
import { avatarEmoji } from "../../lib/avatar";
import type { ExpertSummary, Project, Team } from "../../types";
import { MemberPickerDialog } from "./MemberPicker";
import { TeamPickerDialog } from "./TeamPicker";

/** 新建项目表单：名称/描述/项目指令 + 团队播种(弹框) + 成员(弹框选择+列表删除) + 工作目录。 */
export function NewProjectModal({ onClose, onCreated, notifyErr }: {
  onClose: () => void;
  onCreated: (p: Project) => void;
  notifyErr: (msg: string) => void;
}) {
  const messages = useMessages();
  const [name, setName] = useState("");
  const [desc, setDesc] = useState("");
  const [instructions, setInstructions] = useState("");
  const [members, setMembers] = useState<string[]>([]);
  const [workspaceDir, setWorkspaceDir] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [instructionsOpen, setInstructionsOpen] = useState(false);
  const [memberPickerOpen, setMemberPickerOpen] = useState(false);
  const [confirmingMember, setConfirmingMember] = useState<string | null>(null);
  const [teamPickerOpen, setTeamPickerOpen] = useState(false);

  const [agents, setExperts] = useState<ExpertSummary[]>([]);
  // 从团队播种的成员可能是团队私有专家，不在散装列表里——单独留存其展示信息，
  // 让成员列表能显示名称/头像/职业（与后端 enrich 的全局兜底一致）。
  const [seededMembers, setSeededMembers] = useState<ExpertSummary[]>([]);
  // 记录每名团队播种成员的来源 team id；提交时据此复制为「项目私有副本」（方案C）。
  const [seededFrom, setSeededFrom] = useState<Record<string, string>>({});
  const [teams, setTeams] = useState<Team[]>([]);
  useEffect(() => {
    void listStandaloneExperts().then(setExperts).catch(() => {});
    void listTeams().then(setTeams).catch(() => {});
  }, []);
  const agentByName = useMemo(() => {
    const m = new Map(seededMembers.map((a) => [a.name, a]));
    for (const a of agents) m.set(a.name, a); // 散装优先覆盖
    return m;
  }, [agents, seededMembers]);

  async function seedFromTeam(teamId: string) {
    setTeamPickerOpen(false);
    try {
      const d = await getTeamDetail(teamId);
      const teamName = teams.find((t) => t.id === teamId)?.displayName || teams.find((t) => t.id === teamId)?.name || "该团队";
      if (members.length > 0) {
        const ok = await messages.confirm({
          title: "替换项目成员",
          message: `从团队「${teamName}」导入 ${d.members.length} 名成员？这会替换当前已选择的 ${members.length} 名成员。`,
          tone: "warning",
          confirmText: "导入并替换",
        });
        if (!ok) return;
      }
      setSeededMembers(d.members);
      const next: Record<string, string> = {};
      for (const m of d.members) next[m.name] = teamId;
      setSeededFrom(next);
      setMembers(d.members.map((m) => m.name));
      // 项目指令为空 → 用团队 lead 正文作章程初稿（不覆盖已填）。
      if (!instructions.trim() && d.lead) {
        const detail = await getExpertDetail(d.lead.id).catch(() => null);
        if (detail?.systemPrompt) setInstructions(detail.systemPrompt);
      }
    } catch (err) {
      notifyErr(String(err));
    }
  }

  function removeMember(name: string) {
    setMembers((v) => v.filter((m) => m !== name));
    setConfirmingMember(null);
  }

  async function submit() {
    if (!name.trim() || busy) return;
    setBusy(true);
    try {
      const p = await createProject(name.trim(), desc.trim() || undefined, instructions.trim() || undefined, workspaceDir || undefined);
      for (const expertName of members) {
        const teamId = seededFrom[expertName];
        // 团队播种成员 → 复制为项目私有副本；散装成员 → 按名引用。
        if (teamId) await importTeamMember(p.id, teamId, expertName);
        else await addProjectMember({ projectId: p.id, expertName });
      }
      onCreated(p);
    } catch (err) {
      setBusy(false);
      notifyErr(String(err));
    }
  }

  return (
    <Drawer className="w-[620px] bg-popover text-popover-foreground" open onClose={onClose} title="新建项目">
      <DrawerHeader onClose={onClose}>
        <h2 className="min-w-0 flex-1 truncate text-base font-semibold text-foreground">新建项目</h2>
      </DrawerHeader>

      <div className="flex min-h-0 flex-col bg-popover">
        <div className="min-h-0 flex-1 space-y-4 overflow-auto px-5 py-4">
          <div className="space-y-1">
            <label className="text-[13px] font-medium text-foreground-secondary">项目名称 *</label>
            <input className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary" placeholder="如：内容工作室" value={name} onChange={(e) => setName(e.target.value)} autoFocus />
          </div>

          <div className="space-y-1">
            <label className="text-[13px] font-medium text-foreground-secondary">项目描述</label>
            <input className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary" placeholder="一句话说明这个项目做什么" value={desc} onChange={(e) => setDesc(e.target.value)} />
          </div>

          <div className="space-y-1.5">
            <div className="flex items-center justify-between gap-3">
              <label className="text-[13px] font-medium text-foreground-secondary">项目指令（选填）</label>
              <div className="flex items-center gap-3">
                {teams.length > 0 && (
                  <button type="button" onClick={() => setTeamPickerOpen(true)} className="flex items-center gap-1 text-[13px] text-primary hover:text-foreground">
                    <Users className="h-3.5 w-3.5" aria-hidden="true" /> 从团队中选择
                  </button>
                )}
                <button type="button" onClick={() => setInstructionsOpen(true)} className="flex items-center gap-1 text-[13px] text-primary">
                  <Pencil className="h-3.5 w-3.5" aria-hidden="true" /> 编辑指令
                </button>
              </div>
            </div>
            <button
              type="button"
              onClick={() => setInstructionsOpen(true)}
              className={`flex w-full min-h-[52px] items-start gap-2 rounded-lg border px-3 py-2 text-left transition hover:border-border ${
                instructions.trim()
                  ? "border-border-subtle bg-background"
                  : "border-dashed border-border bg-background/70"
              }`}
            >
              <FileText className="mt-0.5 h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
              <span className={`line-clamp-2 min-w-0 flex-1 text-[13px] leading-5 ${instructions.trim() ? "text-foreground-secondary" : "text-foreground-muted"}`}>
                {instructions.trim() || "未设置项目指令。创建后将使用通用项目经理；点击这里可添加目标、风格、红线和成员调度规则。"}
              </span>
            </button>
          </div>

          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <label className="text-[13px] font-medium text-foreground-secondary">成员 {members.length > 0 ? `· ${members.length}` : ""}</label>
              <button type="button" onClick={() => setMemberPickerOpen(true)} className="flex items-center gap-1 text-[13px] text-primary"><UserPlus className="h-3.5 w-3.5" aria-hidden="true" /> 选择成员</button>
            </div>
            {members.length === 0 ? (
              <p className="rounded-md border border-dashed border-border py-4 text-center text-[13px] text-foreground-muted">还没选成员。点「选择成员」或「从团队中选择」。</p>
            ) : (
              <ul className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                {members.map((expertName) => {
                  const a = agentByName.get(expertName);
                  const label = a?.displayName || a?.name || expertName;
                  return (
                    <li key={expertName} className="group relative flex min-w-0 items-center gap-2 rounded-md border border-border-subtle bg-background px-2.5 py-2">
                      <span className="grid h-7 w-7 shrink-0 place-items-center rounded-md border border-border bg-background text-[14px]">
                        {avatarEmoji(a?.avatar) ? <span aria-hidden="true">{avatarEmoji(a?.avatar)}</span> : <Bot className="h-3.5 w-3.5 text-foreground-muted" aria-hidden="true" />}
                      </span>
                      <div className="min-w-0 flex-1">
                        <span className="block truncate text-[13px] font-medium text-foreground">{label}</span>
                        {a?.profession && <span className="block truncate text-[11px] text-foreground-muted">{a.profession}</span>}
                      </div>
                      <button
                        type="button"
                        title="移除"
                        onClick={() => setConfirmingMember((cur) => (cur === expertName ? null : expertName))}
                        className="ml-auto grid h-6 w-6 shrink-0 place-items-center rounded-md text-destructive opacity-0 transition hover:bg-destructive/10 focus-visible:bg-destructive/10 group-hover:opacity-100 group-focus-within:opacity-100"
                      >
                        <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
                      </button>
                      {confirmingMember === expertName && (
                        <BubbleConfirm
                          title="移除成员？"
                          description={`将从新项目中移除「${label}」。`}
                          confirmText="删除"
                          onCancel={() => setConfirmingMember(null)}
                          onConfirm={() => removeMember(expertName)}
                        />
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
          </div>

          <div className="space-y-1">
            <label className="text-[13px] font-medium text-foreground-secondary">工作目录</label>
            <div className="flex items-center gap-2">
              <Button tone="outline" className="text-[13px]" onClick={() => void pickDirectory().then((d) => { if (d) setWorkspaceDir(d); }).catch(() => {})}>
                <FolderOpen className="h-4 w-4" aria-hidden="true" /> {workspaceDir ? "更换目录" : "选择目录"}
              </Button>
              <span className="min-w-0 flex-1 truncate text-[13px] text-foreground-muted" title={workspaceDir ?? undefined}>
                {workspaceDir ?? "留空则自动创建项目专属目录"}
              </span>
            </div>
          </div>
        </div>

        <div className="flex justify-end gap-2 border-t border-border-subtle px-5 py-3">
          <Button tone="outline" onClick={onClose}>取消</Button>
          <Button tone="primary" onClick={() => void submit()} disabled={!name.trim() || busy}>
            {busy && <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />} 创建
          </Button>
        </div>
      </div>

      {memberPickerOpen && (
        <MemberPickerDialog
          agents={agents}
          initial={members}
          onClose={() => setMemberPickerOpen(false)}
          onConfirm={(names) => { setMembers(names); setMemberPickerOpen(false); }}
        />
      )}
      {teamPickerOpen && (
        <TeamPickerDialog teams={teams} onClose={() => setTeamPickerOpen(false)} onPick={seedFromTeam} />
      )}
      <NewProjectInstructionsDrawer
        open={instructionsOpen}
        value={instructions}
        onChange={setInstructions}
        onClose={() => setInstructionsOpen(false)}
      />
    </Drawer>
  );
}

function NewProjectInstructionsDrawer({
  open,
  value,
  onChange,
  onClose,
}: {
  open: boolean;
  value: string;
  onChange: (value: string) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState(value);

  useEffect(() => {
    if (open) setDraft(value);
  }, [open, value]);

  function save() {
    onChange(draft);
    onClose();
  }

  return (
    <Drawer className="w-[620px] bg-popover text-popover-foreground" open={open} onClose={onClose} title="编辑项目指令">
      <DrawerHeader onClose={onClose}>
        <h2 className="min-w-0 truncate text-base font-semibold text-foreground">编辑项目指令</h2>
      </DrawerHeader>
      <div className="flex min-h-0 flex-col bg-popover">
        <div className="flex min-h-0 flex-1 flex-col px-5 py-4">
          <textarea
            className="min-h-[260px] w-full flex-1 resize-none rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary"
            placeholder="作为项目经理(PM)的章程：目标、风格、红线、如何调度成员。留空则用通用 PM。"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            autoFocus
          />
        </div>
        <div className="flex justify-end gap-2 border-t border-border-subtle px-5 py-3">
          <Button tone="outline" onClick={onClose}>取消</Button>
          <Button tone="primary" onClick={save}>保存</Button>
        </div>
      </div>
    </Drawer>
  );
}
