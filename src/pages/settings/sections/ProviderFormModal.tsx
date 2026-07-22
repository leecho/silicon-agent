import { useState } from "react";
import { Modal, ModalHeader, Button, ButtonGroup, Select } from "../../../components/ui";
import { TextField } from "../../../components/ui/Field";
import type { Provider, ProviderInput, ProviderProtocol } from "../../../types";
import { PROVIDER_PRESETS } from "./providerPresets";

/**
 * 厂商新增/编辑弹窗。
 * - 新增：initial=null，展示预置模板快捷填充。
 * - 编辑：传 initial；apiKey 留空表示保持现有密钥。
 */
export function ProviderFormModal({
  initial,
  onClose,
  onSubmit,
}: {
  initial: Provider | null;
  onClose: () => void;
  onSubmit: (input: ProviderInput) => Promise<void>;
}) {
  return (
    <Modal open onClose={onClose} title={initial ? "编辑厂商" : "添加厂商"}>
      <ModalHeader onClose={onClose}>
        <h2 className="text-base font-semibold text-foreground">
          {initial ? "编辑厂商" : "添加厂商"}
        </h2>
      </ModalHeader>
      <ProviderForm initial={initial} onCancel={onClose} onSubmit={onSubmit} />
    </Modal>
  );
}

export function ProviderForm({
  initial,
  initialValues,
  onCancel,
  onSubmit,
}: {
  initial: Provider | null;
  initialValues?: { name: string; baseUrl: string; protocol?: ProviderProtocol };
  onCancel?: () => void;
  onSubmit: (input: ProviderInput) => Promise<void>;
}) {
  const [name, setName] = useState(initial?.name ?? initialValues?.name ?? "");
  const [baseUrl, setBaseUrl] = useState(initial?.baseUrl ?? initialValues?.baseUrl ?? "");
  const [protocol, setProtocol] = useState<ProviderProtocol>(
    initial?.protocol ?? initialValues?.protocol ?? "openai",
  );
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const hasSecret = initial?.hasSecret ?? false;

  async function handleSave() {
    setSaving(true);
    setError(null);
    try {
      await onSubmit({
        id: initial?.id ?? null,
        name,
        baseUrl,
        apiKey: apiKey === "" ? null : apiKey,
        enabled: initial?.enabled ?? true,
        protocol,
      });
      onCancel?.();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <div className="flex flex-col gap-4 py-4">
        {!initial && !initialValues && (
          <div className="flex flex-wrap gap-2">
            {PROVIDER_PRESETS.map((p) => (
              <button
                key={p.key}
                type="button"
                className="rounded-md border border-border px-3 py-1.5 text-xs text-foreground-secondary transition hover:bg-accent hover:text-foreground"
                onClick={() => {
                  if (p.key !== "custom") {
                    setName(p.name);
                    setBaseUrl(p.baseUrl);
                    setProtocol(p.protocol);
                  } else {
                    setName("");
                    setBaseUrl("");
                    setProtocol("openai");
                  }
                }}
              >
                {p.name}
              </button>
            ))}
          </div>
        )}
        <TextField label="厂商名" value={name} onChange={setName} placeholder="DeepSeek" />
        <TextField
          label="Base URL"
          value={baseUrl}
          onChange={setBaseUrl}
          placeholder="https://api.deepseek.com/v1"
        />
        <label className="flex flex-col gap-1">
          <span className="text-sm font-medium text-foreground">协议</span>
          <Select
            className="w-full"
            value={protocol}
            onChange={(value) => setProtocol(value as ProviderProtocol)}
            options={[
              { value: "openai", label: "OpenAI 兼容" },
              { value: "anthropic", label: "Anthropic（Claude 原生）" },
            ]}
          />
          <span className="text-xs text-foreground-secondary">
            Anthropic 使用原生 Messages API（/v1/messages、x-api-key）。
          </span>
        </label>
        <TextField
          label="API Key"
          description={
            hasSecret
              ? `已保存密钥${initial?.secretHint ? ` · ${initial.secretHint}` : ""}，留空保持现有密钥。`
              : "保存后只回显掩码，明文不会再次显示。"
          }
          type="password"
          autoComplete="off"
          value={apiKey}
          onChange={setApiKey}
          placeholder={hasSecret ? "留空保持现有密钥" : "sk-..."}
        />
        {error && <p className="text-sm text-destructive">{error}</p>}
      </div>
      <ButtonGroup align="end" className="border-t border-border-subtle pt-4">
        {onCancel && (
          <Button tone="outline" onClick={onCancel} disabled={saving}>
            取消
          </Button>
        )}
        <Button tone="primary" onClick={() => void handleSave()} disabled={saving}>
          {saving ? "保存中…" : "保存"}
        </Button>
      </ButtonGroup>
    </>
  );
}
