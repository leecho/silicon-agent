import { useEffect, useMemo, useState } from "react";

import { addProjectMember, listStandaloneExperts } from "../../api";
import { Button } from "../../components/ui/Button";
import type { ExpertSummary } from "../../types";

export function AddMemberModal({
  projectId,
  onClose,
  onAdded,
  notifyErr,
}: {
  projectId: string;
  onClose: () => void;
  onAdded: () => void;
  notifyErr: (msg: string) => void;
}) {
  const [agents, setExperts] = useState<ExpertSummary[]>([]);
  const [expertName, setExpertName] = useState("");
  const [roleLabel, setRoleLabel] = useState("");
  const [responsibilities, setResponsibilities] = useState("");

  useEffect(() => {
    void listStandaloneExperts().then((a) => {
      setExperts(a);
      if (a[0]) setExpertName(a[0].name);
    });
  }, []);

  const selected = useMemo(() => agents.find((a) => a.name === expertName), [agents, expertName]);

  async function submit() {
    if (!expertName) return;
    try {
      await addProjectMember({
        projectId,
        expertName,
        roleLabel: roleLabel.trim() || selected?.profession || null,
        responsibilities: responsibilities.trim() || selected?.description || null,
      });
      onAdded();
    } catch (err) {
      notifyErr(String(err));
    }
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/30" onClick={onClose}>
      <div className="w-[440px] rounded-lg border border-border bg-popover p-4" onClick={(e) => e.stopPropagation()}>
        <h3 className="mb-3 text-base font-semibold text-foreground">添加成员</h3>
        <div className="space-y-2">
          <select className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary" value={expertName} onChange={(e) => setExpertName(e.target.value)}>
            {agents.length === 0 && <option value="">（先到「专家」页创建/加入专家）</option>}
            {agents.map((a) => (
              <option key={a.id} value={a.name}>
                {a.displayName || a.name}
                {a.profession ? ` · ${a.profession}` : ""}
              </option>
            ))}
          </select>
          <input className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary" placeholder="职责标签（如 前端工程师，留空用职业）" value={roleLabel} onChange={(e) => setRoleLabel(e.target.value)} />
          <textarea className="min-h-[60px] w-full resize-y rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary" placeholder="路由依据：这个成员适合接什么（留空用其描述）" value={responsibilities} onChange={(e) => setResponsibilities(e.target.value)} />
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <Button tone="outline" onClick={onClose}>取消</Button>
          <Button tone="primary" onClick={() => void submit()} disabled={!expertName}>添加</Button>
        </div>
      </div>
    </div>
  );
}
