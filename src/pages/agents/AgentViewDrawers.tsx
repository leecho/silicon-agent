import { useEffect, useState } from "react";
import { FolderOpen } from "lucide-react";
import {
  approveSoulProposal,
  listSoulVersions,
  pickDirectory,
  rejectSoulProposal,
  updateAgent,
} from "../../api";
import { Button } from "../../components/ui/Button";
import { Drawer, DrawerHeader } from "../../components/ui/Drawer";
import { EmojiPicker } from "../../components/ui/EmojiPicker";
import { ExpertPickerDialog } from "../../components/experts/ExpertPickerDialog";
import { MarkdownText } from "../../components/ui/MarkdownText";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { Select } from "../../components/ui/Select";
import type { Agent, ExpertSummary, SoulVersion } from "../../types";

export function AgentIdentityEditDrawer({
  agent,
  open,
  onClose,
  onSaved,
}: {
  agent: Agent;
  open: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const notify = useNotifications();
  const [displayName, setDisplayName] = useState(agent.displayName ?? "");
  const [profession, setProfession] = useState(agent.profession ?? "");
  const [avatar, setAvatar] = useState(agent.avatar ?? "");
  const [workingDir, setWorkingDir] = useState<string | null>(agent.workingDir ?? null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setDisplayName(agent.displayName ?? "");
    setProfession(agent.profession ?? "");
    setAvatar(agent.avatar ?? "");
    setWorkingDir(agent.workingDir ?? null);
  }, [agent.avatar, agent.displayName, agent.id, agent.profession, agent.workingDir, open]);

  async function save() {
    setBusy(true);
    try {
      await updateAgent({
        ...agent,
        displayName: displayName.trim() || null,
        profession: profession.trim() || null,
        avatar: avatar.trim() || null,
        workingDir: workingDir?.trim() ? workingDir.trim() : null,
      });
      onSaved();
    } catch (err) {
      notify.notify({ tone: "error", title: "保存失败", message: String(err) });
    } finally {
      setBusy(false);
    }
  }

  async function chooseWorkingDir() {
    const picked = await pickDirectory().catch(() => null);
    if (picked) setWorkingDir(picked);
  }

  return (
    <Drawer width="640px" className="w-[min(560px,94vw)] bg-popover text-popover-foreground" open={open} onClose={onClose} title="编辑">
      <DrawerHeader onClose={onClose}>
        <h2 className="min-w-0 truncate text-base font-semibold text-foreground">
          编辑 <span className="text-[12px] font-normal text-foreground-muted">· {agent.displayName || agent.name}</span>
        </h2>
      </DrawerHeader>
      <div className="min-h-0 space-y-4 overflow-auto bg-popover px-5 py-4">
        <EmojiPicker value={avatar} onChange={setAvatar} />
        <TextField label="名称" value={displayName} onChange={setDisplayName} placeholder={agent.name} />
        <TextField label="描述" value={profession} onChange={setProfession} placeholder="例如 研究伙伴" />
        <div className="space-y-1">
          <label className="text-[13px] font-medium text-foreground-secondary">工作目录</label>
          <div className="flex items-center gap-2">
            <Button tone="outline" className="shrink-0 text-[13px]" onClick={() => void chooseWorkingDir()}>
              <FolderOpen className="h-4 w-4" aria-hidden="true" />
              {workingDir ? "更换目录" : "选择目录"}
            </Button>
            <span className="min-w-0 flex-1 truncate text-[12px] text-foreground-muted" title={workingDir ?? undefined}>
              {workingDir ?? "当前使用智能体默认工作目录"}
            </span>
          </div>
        </div>
      </div>
      <div className="flex justify-end gap-2 border-t border-border-subtle px-5 py-3">
        <Button tone="outline" onClick={onClose} disabled={busy}>取消</Button>
        <Button tone="primary" onClick={() => void save()} disabled={busy}>保存</Button>
      </div>
    </Drawer>
  );
}

export function AgentSourceExpertSwitchDrawer({
  agent,
  experts,
  open,
  onClose,
  onSaved,
}: {
  agent: Agent;
  experts: ExpertSummary[];
  open: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const notify = useNotifications();

  async function save(names: string[]) {
    const nextSourceExpert = names[0] ?? "";
    if (!nextSourceExpert || nextSourceExpert === agent.sourceExpertId) {
      onClose();
      return;
    }
    try {
      await updateAgent({ ...agent, sourceExpertId: nextSourceExpert });
      onSaved();
    } catch (err) {
      notify.notify({ tone: "error", title: "切换专家失败", message: String(err) });
    }
  }

  if (!open) return null;

  return (
    <ExpertPickerDialog
      agents={experts}
      confirmText="选择"
      emptyText="暂无可用的专家。"
      initial={agent.sourceExpertId ? [agent.sourceExpertId] : []}
      onClose={onClose}
      onConfirm={(names) => void save(names)}
      selectionMode="single"
      title="选择专家"
    />
  );
}

export function AgentInstructionsViewDrawer({ agent, open, onClose }: { agent: Agent; open: boolean; onClose: () => void }) {
  const [soulVersions, setSoulVersions] = useState<SoulVersion[]>([]);
  const [selectedSoulVersionId, setSelectedSoulVersionId] = useState("current");

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    listSoulVersions(agent.id)
      .then((versions) => {
        if (cancelled) return;
        const active = versions.find((v) => v.status === "active");
        setSoulVersions(versions);
        setSelectedSoulVersionId(active?.id ?? versions[0]?.id ?? "current");
      })
      .catch(() => {
        if (cancelled) return;
        setSoulVersions([]);
        setSelectedSoulVersionId("current");
      });
    return () => { cancelled = true; };
  }, [agent.id, open]);

  const selectedSoulVersion = soulVersions.find((v) => v.id === selectedSoulVersionId) ?? null;
  const soul = selectedSoulVersion?.soul ?? agent.instructions;
  const soulVersionOptions =
    soulVersions.length === 0
      ? [{ label: "当前人格", value: "current" }]
      : soulVersions.map((version) => ({
          label: `${version.status === "active" ? "当前生效" : version.status === "pending" ? "待批准" : "历史"} · ${version.summary || version.source}`,
          value: version.id,
        }));

  return (
    <Drawer width="640px" className="w-[640px] bg-popover text-popover-foreground" open={open} onClose={onClose} title="查看 SOUL">
      <DrawerHeader onClose={onClose}>
        <div className="flex min-w-0 items-center justify-between gap-3">
          <h2 className="min-w-0 truncate text-base font-semibold text-foreground">
            查看 SOUL <span className="text-[12px] font-normal text-foreground-muted">· {agent.displayName || agent.name}</span>
          </h2>
          <div className="flex shrink-0 items-center gap-2">
            <Select
              className="h-8 w-44 rounded-md text-[12px]"
              value={selectedSoulVersionId}
              onChange={setSelectedSoulVersionId}
              options={soulVersionOptions}
            />
          </div>
        </div>
      </DrawerHeader>
      <div className="min-h-0 overflow-auto bg-popover px-5 py-4">
        {soul.trim() ? (
          <MarkdownText value={soul} className="text-[13px] leading-6 text-foreground-secondary" />
        ) : (
          <p className="text-[12px] text-foreground-muted">未设置人设。</p>
        )}
      </div>
    </Drawer>
  );
}

export function AgentIdentityAnchorViewDrawer({ agent, open, onClose }: { agent: Agent; open: boolean; onClose: () => void }) {
  return (
    <Drawer width="640px" className="w-[640px] bg-popover text-popover-foreground" open={open} onClose={onClose} title="身份锚">
      <DrawerHeader onClose={onClose}>
        <h2 className="min-w-0 truncate text-base font-semibold text-foreground">
          身份锚 <span className="text-[12px] font-normal text-foreground-muted">· {agent.displayName || agent.name}</span>
        </h2>
      </DrawerHeader>
      <div className="min-h-0 overflow-auto bg-popover px-5 py-4">
        {agent.identity?.trim() ? (
          <MarkdownText value={agent.identity} className="text-[13px] leading-6 text-foreground-secondary" />
        ) : (
          <p className="text-[12px] text-foreground-muted">未设置身份锚。</p>
        )}
      </div>
    </Drawer>
  );
}

export function AgentIdentityAnchorEditDrawer({
  agent,
  open,
  onClose,
  onSaved,
}: {
  agent: Agent;
  open: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const notify = useNotifications();
  const [identity, setIdentity] = useState(agent.identity);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) {
      setIdentity(agent.identity);
    }
  }, [agent.id, agent.identity, open]);

  async function save() {
    setBusy(true);
    try {
      await updateAgent({ ...agent, identity });
      onSaved();
    } catch (err) {
      notify.notify({ tone: "error", title: "保存失败", message: String(err) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <Drawer width="640px" className="w-[640px] bg-popover text-popover-foreground" open={open} onClose={onClose} title="编辑身份锚">
      <DrawerHeader onClose={onClose}>
        <h2 className="min-w-0 truncate text-base font-semibold text-foreground">
          编辑身份锚 <span className="text-[12px] font-normal text-foreground-muted">· {agent.displayName || agent.name}</span>
        </h2>
      </DrawerHeader>
      <div className="flex min-h-0 flex-col bg-popover">
        <div className="flex min-h-0 flex-1 flex-col px-5 py-4">
          <div className="flex flex-col gap-1">
            <label className="text-[12px] font-medium text-foreground-secondary">
              身份锚（IDENTITY）<span className="font-normal text-foreground-muted">· 稳定身份/边界，不会被自我演化改动</span>
            </label>
            <textarea
              className="min-h-[220px] w-full resize-none rounded-md border border-border bg-background px-3 py-2 text-sm leading-6 text-foreground outline-none focus:border-primary"
              value={identity}
              onChange={(event) => setIdentity(event.target.value)}
              placeholder="例：你是小研，一名严谨的研究助理。不臆造未经核实的事实。"
              autoFocus
            />
          </div>
        </div>
        <div className="flex justify-end gap-2 border-t border-border-subtle px-5 py-3">
          <Button tone="outline" onClick={onClose} disabled={busy}>取消</Button>
          <Button tone="primary" onClick={() => void save()} disabled={busy}>保存</Button>
        </div>
      </div>
    </Drawer>
  );
}

export function AgentSoulEditDrawer({
  agent,
  open,
  onClose,
  onSaved,
}: {
  agent: Agent;
  open: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const notify = useNotifications();
  const [instructions, setInstructions] = useState(agent.instructions);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) {
      setInstructions(agent.instructions);
    }
  }, [agent.id, agent.instructions, open]);

  async function save() {
    setBusy(true);
    try {
      await updateAgent({ ...agent, instructions });
      onSaved();
    } catch (err) {
      notify.notify({ tone: "error", title: "保存失败", message: String(err) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <Drawer width="640px" className="w-[640px] bg-popover text-popover-foreground" open={open} onClose={onClose} title="编辑人格">
      <DrawerHeader onClose={onClose}>
        <h2 className="min-w-0 truncate text-base font-semibold text-foreground">
          编辑人格 <span className="text-[12px] font-normal text-foreground-muted">· {agent.displayName || agent.name}</span>
        </h2>
      </DrawerHeader>
      <div className="flex min-h-0 flex-col bg-popover">
        <div className="flex min-h-0 flex-1 flex-col px-5 py-4">
          <div className="flex min-h-0 flex-1 flex-col gap-1">
            <label className="text-[12px] font-medium text-foreground-secondary">
              人设（SOUL）<span className="font-normal text-foreground-muted">· 可随相处/演化调整</span>
            </label>
            <textarea
              className="min-h-[220px] w-full flex-1 resize-none rounded-md border border-border bg-background px-3 py-2 text-sm leading-6 text-foreground outline-none focus:border-primary"
              value={instructions}
              onChange={(event) => setInstructions(event.target.value)}
              autoFocus
            />
          </div>
        </div>
        <div className="flex justify-end gap-2 border-t border-border-subtle px-5 py-3">
          <Button tone="outline" onClick={onClose} disabled={busy}>取消</Button>
          <Button tone="primary" onClick={() => void save()} disabled={busy}>保存</Button>
        </div>
      </div>
    </Drawer>
  );
}

export function AgentEvolutionDrawer({
  agent,
  open,
  onClose,
  onSaved,
}: {
  agent: Agent;
  open: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const notify = useNotifications();
  const [versions, setVersions] = useState<SoulVersion[]>([]);
  const [busy, setBusy] = useState(false);

  async function reload() {
    try {
      setVersions(await listSoulVersions(agent.id));
    } catch (err) {
      notify.notify({ tone: "error", title: "加载版本失败", message: String(err) });
    }
  }

  useEffect(() => {
    if (open) {
      void reload();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agent.id, open]);

  async function act(fn: () => Promise<void>, okTitle: string) {
    setBusy(true);
    try {
      await fn();
      await reload();
      onSaved();
      notify.success(okTitle);
    } catch (err) {
      notify.notify({ tone: "error", title: "操作失败", message: String(err) });
    } finally {
      setBusy(false);
    }
  }

  const pending = versions.filter((v) => v.status === "pending");

  return (
    <Drawer width="680px" className="w-[680px] bg-popover text-popover-foreground" open={open} onClose={onClose} title="演化提案">
      <DrawerHeader onClose={onClose}>
        <h2 className="min-w-0 truncate text-base font-semibold text-foreground">
          演化提案 <span className="text-[12px] font-normal text-foreground-muted">· {agent.displayName || agent.name}</span>
        </h2>
      </DrawerHeader>
      <div className="min-h-0 overflow-auto bg-popover px-5 py-4">
        <div>
          <div className="mb-2 text-[12px] font-medium text-foreground-secondary">待批准提案（{pending.length}）</div>
          {pending.length === 0 ? (
            <p className="text-[12px] text-foreground-muted">暂无待批准的人格更新。</p>
          ) : (
            <div className="space-y-3">
              {pending.map((v) => (
                <div key={v.id} className="rounded-lg border border-amber-500/40 bg-amber-500/5 px-3 py-3">
                  <div className="text-[13px] font-medium text-foreground">{v.summary || "人格更新提案"}</div>
                  <MarkdownText value={v.soul} className="mt-2 max-h-48 overflow-auto text-[12px] leading-6 text-foreground-secondary" />
                  <div className="mt-3 flex justify-end gap-2">
                    <Button tone="outline" disabled={busy} onClick={() => void act(() => rejectSoulProposal(v.id), "已拒绝")}>拒绝</Button>
                    <Button tone="primary" disabled={busy} onClick={() => void act(() => approveSoulProposal(agent.id, v.id), "已批准并生效")}>批准</Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </Drawer>
  );
}

function TextField({
  label,
  onChange,
  placeholder,
  value,
}: {
  label: string;
  onChange: (value: string) => void;
  placeholder?: string;
  value: string;
}) {
  return (
    <label className="block space-y-1">
      <span className="text-[13px] font-medium text-foreground-secondary">{label}</span>
      <input
        className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-primary"
        placeholder={placeholder}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  );
}
