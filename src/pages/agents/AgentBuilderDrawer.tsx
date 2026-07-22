import { useEffect, useState } from "react";
import { FileDown, FileText, FolderOpen, Loader2, Pencil } from "lucide-react";
import { createAgent, getExpertDetail, listStandaloneExperts, pickDirectory, updateAgent } from "../../api";
import { ExpertPickerDialog } from "../../components/experts/ExpertPickerDialog";
import { Button } from "../../components/ui/Button";
import { Drawer, DrawerHeader } from "../../components/ui/Drawer";
import { EmojiPicker } from "../../components/ui/EmojiPicker";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { Agent, ExpertSummary } from "../../types";

/** 新建智能体：从一个专家播种（软复制指令+技能），补名字/展示身份/人设指令/专属工作目录。 */
export function AgentBuilderDrawer({
  open,
  onClose,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  onCreated: (agent: Agent) => void;
}) {
  const notify = useNotifications();
  const [experts, setExperts] = useState<ExpertSummary[]>([]);
  const [sourceExpert, setSourceExpert] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [profession, setProfession] = useState("");
  const [avatar, setAvatar] = useState("");
  const [instructions, setInstructions] = useState("");
  const [instructionsTouched, setInstructionsTouched] = useState(false);
  const [workingDir, setWorkingDir] = useState<string | null>(null);
  const [instructionsOpen, setInstructionsOpen] = useState(false);
  const [instructionImportOpen, setInstructionImportOpen] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) return;
    setSourceExpert("");
    setDisplayName("");
    setProfession("");
    setAvatar("");
    setInstructions("");
    setInstructionsTouched(false);
    setWorkingDir(null);
    listStandaloneExperts()
      .then((list) => setExperts(list.filter((x) => x.enabled)))
      .catch(() => setExperts([]));
  }, [open]);

  async function handleImportPrompt(names: string[]) {
    const picked = names[0];
    const expert = experts.find((x) => x.name === picked);
    setInstructionImportOpen(false);
    if (!expert) return;
    try {
      const detail = await getExpertDetail(expert.id);
      setSourceExpert(expert.name);
      setInstructions(detail.systemPrompt);
      setInstructionsTouched(true);
    } catch (err) {
      notify.notify({ tone: "error", title: "导入失败", message: String(err) });
    }
  }

  async function handleCreate() {
    if (!sourceExpert) {
      notify.notify({ tone: "error", title: "请先导入专家提示词", message: "在人设指令中选择专家，并导入该专家的提示词。" });
      return;
    }
    if (!displayName.trim()) {
      notify.notify({ tone: "error", title: "请先填写显示名", message: "给这个智能体起个展示名字" });
      return;
    }
    setSaving(true);
    try {
      let agent = await createAgent(sourceExpert, displayName.trim());
      const patchedInstr = instructionsTouched && instructions.trim() ? instructions : agent.instructions;
      if (profession.trim() || avatar.trim() || workingDir || patchedInstr !== agent.instructions) {
        agent = await updateAgent({
          ...agent,
          profession: profession.trim() || null,
          avatar: avatar.trim() || null,
          instructions: patchedInstr,
          workingDir: workingDir || null,
        });
      }
      onCreated(agent);
    } catch (err) {
      notify.notify({ tone: "error", title: "创建失败", message: String(err) });
    } finally {
      setSaving(false);
    }
  }

  const field = "w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-primary";

  return (
    <Drawer width="620px" className=" bg-popover text-popover-foreground" open={open} onClose={onClose} title="新建智能体">
      <DrawerHeader onClose={onClose}>
        <h2 className="min-w-0 flex-1 truncate text-base font-semibold text-foreground">新建智能体</h2>
      </DrawerHeader>

      <div className="flex min-h-0 flex-col bg-popover">
        <div className="min-h-0 flex-1 space-y-4 overflow-auto px-5 py-4">
                    <div className="space-y-1">
              <EmojiPicker value={avatar} onChange={setAvatar} />
            </div>

          <div className="space-y-1">
            <label className="text-[13px] font-medium text-foreground-secondary">名称 *</label>
            <input className={field} placeholder="如 小研" value={displayName} onChange={(e) => setDisplayName(e.target.value)} />
          </div>

          <div className="flex gap-2">
            <div className="flex-1 space-y-1">
              <label className="text-[13px] font-medium text-foreground-secondary">描述</label>
              <input className={field} placeholder="可选，如 研究伙伴" value={profession} onChange={(e) => setProfession(e.target.value)} />
            </div>
          </div>

          <div className="space-y-1.5">
            <div className="flex items-center justify-between gap-3">
              <label className="text-[13px] font-medium text-foreground-secondary">人设指令 *</label>
              <div className="flex items-center gap-3">
                <button type="button" onClick={() => setInstructionImportOpen(true)} className="flex items-center gap-1 text-[13px] text-primary hover:text-foreground">
                  <FileDown className="h-3.5 w-3.5" aria-hidden="true" /> 从专家导入
                </button>
                <button type="button" onClick={() => setInstructionsOpen(true)} className="flex items-center gap-1 text-[13px] text-primary">
                  <Pencil className="h-3.5 w-3.5" aria-hidden="true" /> 编辑指令
                </button>
              </div>
            </div>
            <button
              type="button"
              onClick={() => setInstructionsOpen(true)}
              className={`flex w-full min-h-[52px] items-start gap-2 rounded-lg border px-3 py-2 text-left transition hover:border-border ${instructions.trim() ? "border-border-subtle bg-background" : "border-dashed border-border bg-background/70"}`}
            >
              <FileText className="mt-0.5 h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
              <span className={`line-clamp-2 min-w-0 flex-1 text-[13px] leading-5 ${instructions.trim() ? "text-foreground-secondary" : "text-foreground-muted"}`}>
                {instructions.trim() || "先从专家导入提示词，再按需要编辑它的人设、语气和边界。"}
              </span>
            </button>
            <p className="text-[12px] text-foreground-muted">
              {sourceExpert ? `已导入专家：${sourceExpert}` : "未选择来源专家。创建智能体前需要导入一次专家提示词。"}
            </p>
          </div>

          <div className="space-y-1">
            <label className="text-[13px] font-medium text-foreground-secondary">工作目录</label>
            <div className="flex items-center gap-2">
              <Button tone="outline" className="text-[13px]" onClick={() => void pickDirectory().then((d) => { if (d) setWorkingDir(d); }).catch(() => {})}>
                <FolderOpen className="h-4 w-4" aria-hidden="true" /> {workingDir ? "更换目录" : "选择目录"}
              </Button>
              <span className="min-w-0 flex-1 truncate text-[13px] text-foreground-muted" title={workingDir ?? undefined}>
                {workingDir ?? "留空则用会话级默认目录"}
              </span>
            </div>
          </div>
        </div>

        <div className="flex justify-end gap-2 border-t border-border-subtle px-5 py-3">
          <Button tone="outline" onClick={onClose} disabled={saving}>取消</Button>
          <Button tone="primary" onClick={() => void handleCreate()} disabled={saving || !sourceExpert || !displayName.trim()}>
            {saving && <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />} 创建
          </Button>
        </div>
      </div>

      <AgentInstructionsDraftDrawer
        open={instructionsOpen}
        value={instructions}
        onChange={(v) => { setInstructions(v); setInstructionsTouched(true); }}
        onClose={() => setInstructionsOpen(false)}
      />
      {instructionImportOpen && (
        <ExpertPickerDialog
          agents={experts}
          confirmText="导入提示词"
          initial={sourceExpert ? [sourceExpert] : []}
          onClose={() => setInstructionImportOpen(false)}
          onConfirm={(names) => void handleImportPrompt(names)}
          selectionMode="single"
          title="选择专家"
        />
      )}
    </Drawer>
  );
}

function AgentInstructionsDraftDrawer({
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
    <Drawer width="640px" className=" bg-popover text-popover-foreground" open={open} onClose={onClose} title="编辑人设指令">
      <DrawerHeader onClose={onClose}>
        <h2 className="min-w-0 truncate text-base font-semibold text-foreground">编辑人设指令</h2>
      </DrawerHeader>
      <div className="flex min-h-0 flex-col bg-popover">
        <div className="flex min-h-0 flex-1 flex-col px-5 py-4">
          <textarea
            className="min-h-[260px] w-full flex-1 resize-none rounded-md border border-border bg-background px-3 py-2 text-sm leading-6 outline-none focus:border-primary"
            placeholder="留空则承袭来源专家指令。可写它的语气、专长、边界。"
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
