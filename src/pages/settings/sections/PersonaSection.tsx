import { useEffect, useState } from "react";
import { getAgentPersona, setAgentPersona } from "../../../api";
import { useNotifications } from "../../../components/ui";

/** 人设 section：Agent 身份 + 灵魂编辑。 */
export function PersonaSection() {
  const notify = useNotifications();
  const [identity, setIdentity] = useState("");
  const [soul, setSoul] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let alive = true;
    void getAgentPersona()
      .then((p) => {
        if (!alive) return;
        setIdentity(p.identity);
        setSoul(p.soul);
      })
      .catch(() => {})
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);

  async function save() {
    setSaving(true);
    try {
      await setAgentPersona(identity, soul);
      notify.success("人设已保存。");
    } catch (err) {
      notify.error({ title: "保存人设失败", message: String(err) });
    } finally {
      setSaving(false);
    }
  }

  if (loading) {
    return (
      <section className="grid gap-8" aria-label="人设">
        <p className="text-sm text-foreground-muted">加载中…</p>
      </section>
    );
  }

  return (
    <section className="grid gap-8" aria-label="人设">
      <div className="settings-section-surface overflow-hidden rounded-lg border border-border bg-surface">
        <div className="border-b border-border px-4 py-4 last:border-b-0">
          <label className="mb-1 block text-sm font-medium text-foreground" htmlFor="persona-identity">
            身份
          </label>
          <p className="mb-2 text-xs text-foreground-muted">
            Agent 是谁——名字、角色定位。填写后会完全替换默认人设句；留空则用默认。
          </p>
          <textarea
            id="persona-identity"
            className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition focus:border-ring resize-none"
            rows={3}
            value={identity}
            placeholder="例如：你是小硅，一名严谨细致的研究助手。"
            onChange={(e) => setIdentity(e.target.value)}
          />
        </div>
        <div className="border-b border-border px-4 py-4 last:border-b-0">
          <label className="mb-1 block text-sm font-medium text-foreground" htmlFor="persona-soul">
            灵魂
          </label>
          <p className="mb-2 text-xs text-foreground-muted">
            Agent 的性格、价值观、语气与行事原则。填写后作为「人格」追加到系统提示。
          </p>
          <textarea
            id="persona-soul"
            className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition focus:border-ring resize-none"
            rows={5}
            value={soul}
            placeholder="例如：耐心、克制；动手前先确认需求；不臆测，证据优先。"
            onChange={(e) => setSoul(e.target.value)}
          />
        </div>
      </div>
      <div>
        <button
          type="button"
          disabled={saving}
          onClick={() => void save()}
          className="rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-50"
        >
          {saving ? "保存中…" : "保存"}
        </button>
      </div>
    </section>
  );
}
