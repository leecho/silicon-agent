import { useMemo, useState } from "react";
import { Badge, Modal, ModalHeader, Button, ButtonGroup, Checkbox } from "../../../components/ui";
import { fetchProviderModels } from "../../../api";

type ModelAddPanelProps = {
  providerId: string;
  existingModels?: string[];
  onCancel: () => void;
  onAdd: (models: string[]) => Promise<void>;
};

function parseModelNames(value: string) {
  return value
    .split(/[\n,]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

/** 给某厂商添加模型：支持自动拉取勾选，也支持手动批量粘贴。 */
export function ModelAddPanel({
  providerId,
  existingModels = [],
  onCancel,
  onAdd,
}: ModelAddPanelProps) {
  const [manual, setManual] = useState("");
  const [fetched, setFetched] = useState<string[]>([]);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const existingModelSet = useMemo(
    () => new Set(existingModels.map((model) => model.trim()).filter(Boolean)),
    [existingModels],
  );
  const manualNames = useMemo(() => parseModelNames(manual), [manual]);
  const pendingNames = useMemo(() => {
    const names = new Set<string>();
    picked.forEach((name) => {
      if (!existingModelSet.has(name)) names.add(name);
    });
    manualNames.forEach((name) => {
      if (!existingModelSet.has(name)) names.add(name);
    });
    return [...names];
  }, [existingModelSet, manualNames, picked]);
  const filteredFetched = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    if (!normalizedQuery) return fetched;
    return fetched.filter((id) => id.toLowerCase().includes(normalizedQuery));
  }, [fetched, query]);

  async function handleFetch() {
    setLoading(true);
    setError(null);
    try {
      const ids = await fetchProviderModels(providerId);
      setFetched([...new Set(ids)]);
      if (ids.length === 0) setError("未拉取到模型，可手工输入。");
    } catch (err) {
      setError(`拉取失败：${String(err)}，可手工输入。`);
    } finally {
      setLoading(false);
    }
  }

  function toggle(id: string) {
    if (existingModelSet.has(id)) return;
    setPicked((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function handleSave() {
    const allNames = new Set([...picked, ...manualNames]);
    if (allNames.size === 0) {
      setError("请至少选择或输入一个模型。");
      return;
    }
    if (pendingNames.length === 0) {
      setError("这些模型已存在，无需重复添加。");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await onAdd(pendingNames);
      onCancel();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="text-sm font-semibold text-foreground">添加模型</div>
          <p className="mt-1 text-xs text-foreground-muted">从厂商接口拉取模型列表，或直接粘贴模型 ID。</p>
        </div>
        <Button tone="outline" onClick={() => void handleFetch()} disabled={loading}>
          {loading ? "拉取中..." : "自动拉取模型列表"}
        </Button>
      </div>

      {fetched.length > 0 && (
        <div className="flex flex-col gap-2">
          <input
            className="h-9 rounded-lg border border-border bg-surface px-3 text-sm text-foreground outline-none transition placeholder:text-foreground-muted focus:border-ring"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="筛选已拉取的模型"
          />
          <div className="max-h-56 overflow-auto rounded-lg border border-border bg-surface p-1">
            {filteredFetched.length === 0 ? (
              <p className="px-3 py-3 text-sm text-foreground-muted">没有匹配的模型。</p>
            ) : (
              filteredFetched.map((id) => {
                const exists = existingModelSet.has(id);
                return (
                  <div
                    key={id}
                    className="flex min-h-10 items-center gap-2 rounded-md px-2 py-1.5 text-sm text-foreground transition hover:bg-accent"
                  >
                    <Checkbox
                      aria-label={`选择模型 ${id}`}
                      checked={picked.has(id)}
                      disabled={exists}
                      onChange={() => toggle(id)}
                    />
                    <span className="min-w-0 flex-1 truncate">{id}</span>
                    {exists && <Badge tone="neutral">已添加</Badge>}
                  </div>
                );
              })
            )}
          </div>
        </div>
      )}

      <label className="flex flex-col gap-2">
        <span className="text-sm font-medium text-foreground">手动输入</span>
        <textarea
          className="min-h-24 resize-y rounded-lg border border-border bg-surface px-3 py-2 text-sm text-foreground outline-none transition placeholder:text-foreground-muted focus:border-ring"
          value={manual}
          onChange={(event) => setManual(event.target.value)}
          placeholder="deepseek-chat, deepseek-reasoner"
        />
        <span className="text-xs text-foreground-muted">多个模型可用逗号或换行分隔。</span>
      </label>

      {error && <p className="text-sm text-destructive">{error}</p>}

      <ButtonGroup align="end" className="border-t border-border-subtle pt-4">
        <Button tone="outline" onClick={onCancel} disabled={saving}>
          取消
        </Button>
        <Button tone="primary" onClick={() => void handleSave()} disabled={saving}>
          {saving ? "保存中..." : pendingNames.length > 0 ? `添加 ${pendingNames.length} 个模型` : "添加模型"}
        </Button>
      </ButtonGroup>
    </div>
  );
}

export function ModelAddModal({
  providerId,
  existingModels,
  onClose,
  onAdd,
}: {
  providerId: string;
  existingModels?: string[];
  onClose: () => void;
  onAdd: (models: string[]) => Promise<void>;
}) {
  return (
    <Modal open onClose={onClose} title="添加模型">
      <ModalHeader onClose={onClose}>
        <h2 className="text-base font-semibold text-foreground">添加模型</h2>
      </ModalHeader>
      <div className="flex flex-col gap-4 py-4">
        <ModelAddPanel
          providerId={providerId}
          existingModels={existingModels}
          onCancel={onClose}
          onAdd={onAdd}
        />
      </div>
    </Modal>
  );
}
