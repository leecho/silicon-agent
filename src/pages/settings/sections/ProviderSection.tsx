import { useCallback, useEffect, useMemo, useState, type Dispatch, type ReactNode, type SetStateAction } from "react";
import { ArrowLeft, ChevronRight, Cpu, Eye, EyeOff, KeyRound, Pencil, Plus, Shuffle, Sparkles, Trash2, Wifi } from "lucide-react";
import {
  deleteProvider,
  deleteProviderModel,
  fetchProviderModels,
  getAuxModelId,
  getFallbackModel,
  listProviders,
  listProviderModels,
  setAuxModelId,
  setDefaultModel,
  setFallbackModel,
  setModelEnabled,
  setProviderEnabled,
  testProvider,
  upsertProvider,
  upsertProviderModel,
} from "../../../api";
import type { ModelEntry, Provider, ProviderInput } from "../../../types";
import { Badge, Button, Drawer, DrawerHeader, Select, Switch, Tooltip, useMessages, useNotifications } from "../../../components/ui";
import { SettingItem } from "../../../components/settings/SettingsControls";
import { ProviderForm, ProviderFormModal } from "./ProviderFormModal";
import { buildProviderCatalogRows, type CustomProviderCatalogRow, type ProviderCatalogRow } from "./providerCatalog";
import { PROVIDER_PRESETS, type ProviderPreset } from "./providerPresets";

/** 厂商详情页的模型行：实时拉取的模型名 ∪ 已有 ModelEntry。 */
type DetailModelRow = {
  model: string;
  entry: ModelEntry | null;
  enabled: boolean;
};

function parseModelNames(value: string): string[] {
  const seen = new Set<string>();
  return value
    .split(/[\n,]/)
    .map((item) => item.trim())
    .filter((item) => item && !seen.has(item) && (seen.add(item), true));
}

/** 合并实时拉取的模型名与已有记录，拉取顺序优先，DB 独有的（陈旧/手动）追加末尾。 */
function buildModelRows(fetched: string[], dbModels: ModelEntry[]): DetailModelRow[] {
  const byName = new Map(dbModels.map((model) => [model.model, model] as const));
  const seen = new Set<string>();
  const rows: DetailModelRow[] = [];
  for (const name of fetched) {
    if (seen.has(name)) continue;
    seen.add(name);
    const entry = byName.get(name) ?? null;
    rows.push({ model: name, entry, enabled: entry?.enabled ?? false });
  }
  for (const model of dbModels) {
    if (seen.has(model.model)) continue;
    seen.add(model.model);
    rows.push({ model: model.model, entry: model, enabled: model.enabled });
  }
  return rows;
}

type ProviderDetailTarget = {
  providerPresetKey?: string | null;
  providerId?: string | null;
};

type ProviderSectionProps = ProviderDetailTarget & {
  onOpenProvider?: (target: ProviderDetailTarget) => void;
  onBackToCatalog?: () => void;
};

/** 多模型设置：预置厂商目录 → 厂商详情 → 厂商配置与模型配置。 */
export function ProviderSection({
  providerPresetKey = null,
  providerId = null,
  onOpenProvider,
  onBackToCatalog,
}: ProviderSectionProps = {}) {
  const notify = useNotifications();
  const message = useMessages();
  const [providers, setProviders] = useState<Provider[]>([]);
  const [modelsByProvider, setModelsByProvider] = useState<Record<string, ModelEntry[]>>({});
  const [editing, setEditing] = useState<Provider | null | "new">(null);
  const [providerConfigOpen, setProviderConfigOpen] = useState(false);
  const [testingId, setTestingId] = useState<string | null>(null);
  const [localDetail, setLocalDetail] = useState<ProviderDetailTarget>({});
  const [fetchedModels, setFetchedModels] = useState<string[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);

  const activePresetKey = providerPresetKey ?? localDetail.providerPresetKey ?? null;
  const activeProviderId = providerId ?? localDetail.providerId ?? null;

  const reloadModels = useCallback(async (targetProviderId: string) => {
    const models = await listProviderModels(targetProviderId);
    setModelsByProvider((prev) => ({ ...prev, [targetProviderId]: models }));
  }, []);

  const loadFetchedModels = useCallback(async (targetProviderId: string) => {
    setModelsLoading(true);
    setModelsError(null);
    try {
      const ids = await fetchProviderModels(targetProviderId);
      setFetchedModels([...new Set(ids)]);
    } catch (err) {
      setFetchedModels([]);
      setModelsError(String(err));
    } finally {
      setModelsLoading(false);
    }
  }, []);

  const reloadAll = useCallback(async () => {
    const ps = await listProviders();
    setProviders(ps);
    const entries = await Promise.all(
      ps.map(async (provider) => [provider.id, await listProviderModels(provider.id)] as const),
    );
    setModelsByProvider(Object.fromEntries(entries));
  }, []);

  useEffect(() => {
    reloadAll().catch((err) => notify.error({ title: "加载失败", message: String(err) }));
  }, [reloadAll, notify]);

  const catalogRows = useMemo(
    () => buildProviderCatalogRows(PROVIDER_PRESETS, providers, modelsByProvider),
    [modelsByProvider, providers],
  );
  const allModels = useMemo(
    () => providers.flatMap((provider) => modelsByProvider[provider.id] ?? []),
    [modelsByProvider, providers],
  );
  const defaultModel = useMemo(
    () => allModels.find((model) => model.isDefault) ?? null,
    [allModels],
  );
  const providerNamesById = useMemo(
    () => Object.fromEntries(providers.map((provider) => [provider.id, provider.name] as const)),
    [providers],
  );

  const selected = useMemo(() => {
    const presetRow =
      (activePresetKey
        ? catalogRows.presetRows.find((row) => row.preset.key === activePresetKey)
        : null) ??
      (activeProviderId
        ? catalogRows.presetRows.find((row) => row.provider?.id === activeProviderId)
        : null) ??
      null;
    const customRow = activeProviderId
      ? catalogRows.customRows.find((row) => row.provider.id === activeProviderId) ?? null
      : null;
    const provider = presetRow?.provider ?? customRow?.provider ?? null;
    const preset =
      presetRow?.preset ??
      (activePresetKey ? PROVIDER_PRESETS.find((item) => item.key === activePresetKey) ?? null : null);
    return { presetRow, customRow, provider, preset };
  }, [activePresetKey, activeProviderId, catalogRows.customRows, catalogRows.presetRows]);

  const showingDetail = Boolean(activePresetKey || activeProviderId);
  const detailProviderId = selected.provider?.id ?? null;

  const modelRows = useMemo(
    () => buildModelRows(fetchedModels, detailProviderId ? modelsByProvider[detailProviderId] ?? [] : []),
    [fetchedModels, detailProviderId, modelsByProvider],
  );

  // 进入厂商详情时实时拉取该厂商的可用模型列表。
  useEffect(() => {
    if (!detailProviderId) {
      setFetchedModels([]);
      setModelsError(null);
      setModelsLoading(false);
      return;
    }
    void loadFetchedModels(detailProviderId);
  }, [detailProviderId, loadFetchedModels]);

  function openProvider(target: ProviderDetailTarget) {
    setProviderConfigOpen(false);
    if (onOpenProvider) {
      onOpenProvider(target);
    } else {
      setLocalDetail(target);
    }
  }

  function backToCatalog() {
    setProviderConfigOpen(false);
    if (onBackToCatalog) {
      onBackToCatalog();
    } else {
      setLocalDetail({});
    }
  }

  async function saveProvider(input: ProviderInput) {
    const saved = await upsertProvider(input);
    await reloadAll();
    notify.success("厂商已保存。");
    if (!input.id) {
      openProvider({
        providerPresetKey: selected.preset?.key ?? null,
        providerId: saved.id,
      });
    }
  }

  async function removeProvider(provider: Provider) {
    const confirmed = await message.confirm({
      title: "删除厂商",
      message: `删除厂商「${provider.name}」及其下所有模型？`,
      confirmText: "删除",
      cancelText: "取消",
      tone: "warning",
    });
    if (!confirmed) return;
    try {
      await deleteProvider(provider.id);
      await reloadAll();
      backToCatalog();
      notify.success("厂商已删除。");
    } catch (err) {
      notify.error({ title: "删除失败", message: String(err) });
      await reloadAll();
    }
  }

  async function toggleProvider(provider: Provider, enabled: boolean) {
    try {
      await setProviderEnabled(provider.id, enabled);
      await reloadAll();
    } catch (err) {
      notify.error({ title: "启用切换失败", message: String(err) });
      await reloadAll();
    }
  }

  async function handleTest(provider: Provider) {
    setTestingId(provider.id);
    try {
      const result = await testProvider(provider.id);
      if (result.status === "ready") notify.success(result.detail);
      else notify.error({ title: "连通失败", message: result.detail });
      await reloadAll();
    } catch (err) {
      notify.error({ title: "连通失败", message: String(err) });
    } finally {
      setTestingId(null);
    }
  }

  async function addModels(targetProviderId: string, names: string[]) {
    const results = await Promise.allSettled(
      names.map((model) =>
        upsertProviderModel({
          id: null,
          providerId: targetProviderId,
          model,
          displayName: null,
          enabled: true,
        }),
      ),
    );
    await reloadModels(targetProviderId);
    const ok = results.filter((result) => result.status === "fulfilled").length;
    const failed = results.length - ok;
    if (ok > 0) notify.success(`已添加 ${ok} 个模型。`);
    if (failed > 0) notify.error({ title: "部分添加失败", message: `${failed} 个模型添加失败` });
  }

  async function promptAddModel(targetProviderId: string) {
    const next = await message.prompt({
      title: "添加模型",
      message: "输入模型名称，多个可用逗号或换行分隔。",
      placeholder: "deepseek-chat, deepseek-reasoner",
      confirmText: "添加",
      cancelText: "取消",
    });
    if (next === null) return;
    const names = parseModelNames(next);
    if (names.length === 0) {
      notify.error("请至少输入一个模型名称。");
      return;
    }
    await addModels(targetProviderId, names);
  }

  async function toggleModelRow(targetProviderId: string, row: DetailModelRow, enabled: boolean) {
    try {
      if (row.entry) {
        await setModelEnabled(row.entry.id, enabled);
      } else if (enabled) {
        await upsertProviderModel({
          id: null,
          providerId: targetProviderId,
          model: row.model,
          displayName: null,
          enabled: true,
        });
      }
      await reloadModels(targetProviderId);
    } catch (err) {
      notify.error({ title: "启用切换失败", message: String(err) });
      await reloadModels(targetProviderId);
    }
  }

  async function saveContextLimit(model: ModelEntry, raw: string) {
    const trimmed = raw.trim();
    const n = trimmed === "" ? null : Math.floor(Number(trimmed));
    if (n === model.contextLimit) return;
    if (n !== null && (!Number.isFinite(n) || n <= 0)) {
      notify.error("上下文上限需为正整数（留空用内置默认）");
      await reloadModels(model.providerId);
      return;
    }
    try {
      await upsertProviderModel({
        id: model.id,
        providerId: model.providerId,
        model: model.model,
        displayName: model.displayName,
        enabled: model.enabled,
        contextLimit: n,
        // 保留 vision 覆盖（原始值），避免被本次保存清空。
        supportsVision: model.supportsVision,
      });
      await reloadModels(model.providerId);
    } catch (err) {
      notify.error({ title: "保存上下文上限失败", message: String(err) });
      await reloadModels(model.providerId);
    }
  }

  /// vision 二态切换：以当前生效值（覆盖优先，否则探测能力）为准，点击切换为相反的显式覆盖。
  async function toggleVision(model: ModelEntry) {
    const effective = model.supportsVision ?? model.visionCapable;
    const next = !effective;
    try {
      await upsertProviderModel({
        id: model.id,
        providerId: model.providerId,
        model: model.model,
        displayName: model.displayName,
        enabled: model.enabled,
        // 保留上下文上限覆盖（原始值），避免被本次保存清空。
        contextLimit: model.contextLimit,
        supportsVision: next,
      });
      await reloadModels(model.providerId);
    } catch (err) {
      notify.error({ title: "保存图像能力失败", message: String(err) });
      await reloadModels(model.providerId);
    }
  }

  async function promptContextLimit(model: ModelEntry) {
    const next = await message.prompt({
      title: "设置模型上下文",
      message: (
        <span>
          为「{model.displayName || model.model}」设置上下文窗口上限。留空使用内置默认值。
        </span>
      ),
      defaultValue: model.contextLimit == null ? "" : String(model.contextLimit),
      placeholder: "例如 128000，留空用默认",
      confirmText: "保存",
      cancelText: "取消",
    });
    if (next === null) return;
    await saveContextLimit(model, next);
  }

  async function makeDefault(model: ModelEntry) {
    try {
      await setDefaultModel(model.id);
      await reloadAll();
      notify.success(`已设默认模型：${model.model}`);
    } catch (err) {
      notify.error({ title: "设默认失败", message: String(err) });
      await reloadAll();
    }
  }

  async function removeModel(model: ModelEntry) {
    const confirmed = await message.confirm({
      title: "删除模型",
      message: (
        <span>
          确认删除模型「{model.displayName || model.model}」吗？
          {model.isDefault ? "该模型当前是默认模型，删除后需要重新设置默认模型。" : ""}
        </span>
      ),
      confirmText: "删除",
      cancelText: "取消",
      tone: "warning",
    });
    if (!confirmed) return;
    try {
      await deleteProviderModel(model.id);
      await reloadModels(model.providerId);
    } catch (err) {
      notify.error({ title: "删除失败", message: String(err) });
      await reloadModels(model.providerId);
    }
  }

  function formatContextLimit(limit: number | null | undefined) {
    return limit == null ? "默认" : `${limit.toLocaleString()} tokens`;
  }

  if (showingDetail) {
    return (
      <div className="flex flex-col gap-6">
        <GlobalDefaultModelPanel
          defaultModel={defaultModel}
          models={allModels}
          onSetDefault={makeDefault}
          providerNamesById={providerNamesById}
        />
        <ProviderDetailView
          modelRows={modelRows}
          modelsLoading={modelsLoading}
          modelsError={modelsError}
          onAddModel={promptAddModel}
          onBack={backToCatalog}
          onDeleteModel={removeModel}
          onDeleteProvider={removeProvider}
          onRetryFetch={loadFetchedModels}
          onSaveProvider={saveProvider}
          onSetProviderConfigOpen={setProviderConfigOpen}
          onSetContextLimit={promptContextLimit}
          onToggleVision={toggleVision}
          onTestProvider={handleTest}
          onToggleModel={toggleModelRow}
          onToggleProvider={toggleProvider}
          preset={selected.preset}
          provider={selected.provider}
          providerConfigOpen={providerConfigOpen}
          testingId={testingId}
          formatContextLimit={formatContextLimit}
        />
      </div>
    );
  }

  return (
    <ProviderCatalogView
      customRows={catalogRows.customRows}
      defaultModel={defaultModel}
      models={allModels}
      onAddCustom={() => setEditing("new")}
      onOpenProvider={openProvider}
      onSetDefault={makeDefault}
      presetRows={catalogRows.presetRows}
      providerNamesById={providerNamesById}
    >
      {editing !== null && (
        <ProviderFormModal
          initial={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
          onSubmit={saveProvider}
        />
      )}
    </ProviderCatalogView>
  );
}

function ProviderCatalogView({
  children,
  customRows,
  defaultModel,
  models,
  onAddCustom,
  onOpenProvider,
  onSetDefault,
  presetRows,
  providerNamesById,
}: {
  children: ReactNode;
  customRows: CustomProviderCatalogRow[];
  defaultModel: ModelEntry | null;
  models: ModelEntry[];
  onAddCustom: () => void;
  onOpenProvider: (target: ProviderDetailTarget) => void;
  onSetDefault: (model: ModelEntry) => Promise<void>;
  presetRows: ProviderCatalogRow[];
  providerNamesById: Record<string, string>;
}) {
  return (
    <section className="flex flex-col gap-6" aria-label="多模型配置">
      <GlobalDefaultModelPanel
        defaultModel={defaultModel}
        models={models}
        onSetDefault={onSetDefault}
        providerNamesById={providerNamesById}
      />

      <ModelRolesPanel models={models} providerNamesById={providerNamesById} />

      <div className="flex items-center justify-between gap-3">
        <h3 className="text-[15px] font-[650] leading-[1.4] text-foreground">模型厂商</h3>
        <Button tone="outline" onClick={onAddCustom}>
          <Plus className="h-4 w-4" /> 添加自定义厂商
        </Button>
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        {presetRows.map((row) => (
          <ProviderPresetCard
            key={row.preset.key}
            row={row}
            onOpen={() =>
              onOpenProvider({
                providerPresetKey: row.preset.key,
                providerId: row.provider?.id ?? null,
              })
            }
          />
        ))}
      </div>

      {customRows.length > 0 && (
        <div className="flex flex-col gap-3">
          <h3 className="text-[15px] font-[650] leading-[1.4] text-foreground">自定义厂商</h3>
          <div className="grid gap-3 md:grid-cols-2">
            {customRows.map((row) => (
              <CustomProviderCard
                key={row.provider.id}
                row={row}
                onOpen={() => onOpenProvider({ providerId: row.provider.id })}
              />
            ))}
          </div>
        </div>
      )}
      {children}
    </section>
  );
}

function ProviderPresetCard({ row, onOpen }: { row: ProviderCatalogRow; onOpen: () => void }) {
  const provider = row.provider;
  return (
    <button
      className="group grid min-h-[176px] grid-rows-[auto_minmax(0,1fr)_auto] rounded-lg border border-border bg-surface p-4 text-left transition hover:bg-accent focus:border-ring focus:outline-none"
      type="button"
      onClick={onOpen}
    >
      <ProviderCardHeader
        name={row.preset.name}
        subtitle={provider?.baseUrl ?? row.preset.baseUrl}
        configured={row.configured}
        enabled={provider?.enabled ?? false}
      />
      <div className="mt-4 flex flex-wrap gap-2">
        <Badge tone={row.configured ? "success" : "neutral"}>
          {row.configured ? "密钥已配置" : "未配置"}
        </Badge>
        {provider && <Badge tone={provider.enabled ? "success" : "neutral"}>{provider.enabled ? "已启用" : "已停用"}</Badge>}
        {provider?.lastCheck && (
          <Tooltip content={provider.lastCheck.detail}>
            <Badge tone={provider.lastCheck.status === "ready" ? "success" : "danger"}>
              {provider.lastCheck.status === "ready" ? provider.lastCheck.detail : "连通失败"}
            </Badge>
          </Tooltip>
        )}
      </div>
      <ProviderStats
        enabledModelCount={row.enabledModelCount}
        modelCount={row.modelCount}
      />
    </button>
  );
}

function CustomProviderCard({ row, onOpen }: { row: CustomProviderCatalogRow; onOpen: () => void }) {
  return (
    <button
      className="group grid min-h-[156px] grid-rows-[auto_minmax(0,1fr)_auto] rounded-lg border border-border bg-surface p-4 text-left transition hover:bg-accent focus:border-ring focus:outline-none"
      type="button"
      onClick={onOpen}
    >
      <ProviderCardHeader
        name={row.provider.name}
        subtitle={row.provider.baseUrl}
        configured={row.configured}
        enabled={row.provider.enabled}
      />
      <div className="mt-4 flex flex-wrap gap-2">
        <Badge tone={row.configured ? "success" : "neutral"}>
          {row.configured ? "密钥已配置" : "未配置"}
        </Badge>
        <Badge tone={row.provider.enabled ? "success" : "neutral"}>
          {row.provider.enabled ? "已启用" : "已停用"}
        </Badge>
      </div>
      <ProviderStats
        enabledModelCount={row.enabledModelCount}
        modelCount={row.modelCount}
      />
    </button>
  );
}

function ProviderCardHeader({
  configured,
  enabled,
  name,
  subtitle,
}: {
  configured: boolean;
  enabled: boolean;
  name: string;
  subtitle: string;
}) {
  return (
    <div className="flex min-w-0 items-start justify-between gap-3">
      <div className="flex min-w-0 items-center gap-3">
        <span className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-muted text-foreground-secondary">
          {configured ? <KeyRound className="h-5 w-5" aria-hidden="true" /> : <Cpu className="h-5 w-5" aria-hidden="true" />}
        </span>
        <div className="min-w-0">
          <div className="truncate text-base font-semibold text-foreground">{name}</div>
          <div className="mt-1 truncate text-xs text-foreground-muted">{subtitle || "自定义 OpenAI-compatible 地址"}</div>
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {enabled && <span className="h-2 w-2 rounded-full bg-status-success-text" aria-label="已启用" />}
        <ChevronRight className="h-4 w-4 text-foreground-muted transition group-hover:translate-x-0.5 group-hover:text-foreground" aria-hidden="true" />
      </div>
    </div>
  );
}

function ProviderStats({ enabledModelCount, modelCount }: { enabledModelCount: number; modelCount: number }) {
  return (
    <div className="mt-4 grid grid-cols-2 gap-2 border-t border-border pt-3">
      <div className="text-center">
        <div className="text-lg font-semibold text-foreground">{modelCount}</div>
        <div className="mt-0.5 text-xs text-foreground-muted">已配置模型</div>
      </div>
      <div className="text-center">
        <div className="text-lg font-semibold text-foreground">{enabledModelCount}</div>
        <div className="mt-0.5 text-xs text-foreground-muted">启用中</div>
      </div>
    </div>
  );
}

function GlobalDefaultModelPanel({
  defaultModel,
  models,
  onSetDefault,
  providerNamesById,
}: {
  defaultModel: ModelEntry | null;
  models: ModelEntry[];
  onSetDefault: (model: ModelEntry) => Promise<void>;
  providerNamesById: Record<string, string>;
}) {
  const enabledModels = models.filter((model) => model.enabled);
  const options = enabledModels.map((model) => {
    const providerName = providerNamesById[model.providerId] ?? "未知厂商";
    const label = model.displayName || model.model;
    return {
      value: model.id,
      label,
      description: providerName,
      group: providerName,
      searchText: `${providerName} ${label} ${model.model}`,
    };
  });

  return (
    <section className="rounded-lg border border-border bg-surface px-5 py-4" aria-label="全局默认模型">
      <div className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
        <div className="min-w-0">
          <h3 className="text-sm font-semibold text-foreground">全局默认模型</h3>
          <p className="mt-1 text-xs text-foreground-muted">会话未单独选择模型时使用该模型。</p>
        </div>
        <Select
          className="w-full text-sm md:w-72"
          disabled={options.length === 0}
          onChange={(modelId) => {
            const model = enabledModels.find((item) => item.id === modelId);
            if (model) void onSetDefault(model);
          }}
          options={options}
          searchable
          searchPlaceholder="筛选模型"
          tooltip={options.length === 0 ? "先在厂商详情中添加并启用模型" : undefined}
          value={defaultModel?.id ?? ""}
        />
      </div>
    </section>
  );
}

/** 模型角色：备用模型（fallback）与辅助模型，作用于全局，落在「模型配置」首页。 */
function ModelRolesPanel({
  models,
  providerNamesById,
}: {
  models: ModelEntry[];
  providerNamesById: Record<string, string>;
}) {
  const notify = useNotifications();
  const [fallbackId, setFallbackId] = useState<string | null>(null);
  const [auxId, setAuxId] = useState<string | null>(null);

  useEffect(() => {
    getFallbackModel().then(setFallbackId).catch(() => {});
    getAuxModelId().then(setAuxId).catch(() => {});
  }, []);

  const enabledModelOptions = useMemo(
    () =>
      models
        .filter((model) => model.enabled)
        .map((model) => {
          const providerName = providerNamesById[model.providerId] ?? "未知厂商";
          const label = model.displayName || model.model;
          return {
            value: model.id,
            label,
            description: model.displayName ? model.model : undefined,
            group: providerName,
            searchText: `${providerName} ${label} ${model.model}`,
          };
        }),
    [models, providerNamesById],
  );
  const fallbackOptions = useMemo(
    () => [{ label: "不使用备用模型", value: "" }, ...enabledModelOptions],
    [enabledModelOptions],
  );
  const auxOptions = useMemo(
    () => [{ label: "跟随会话模型（默认）", value: "" }, ...enabledModelOptions],
    [enabledModelOptions],
  );

  async function changeFallback(modelId: string) {
    const next = modelId === "" ? null : modelId;
    const prev = fallbackId;
    setFallbackId(next);
    try {
      await setFallbackModel(next);
    } catch (err) {
      notify.error({ title: "备用模型设置失败", message: String(err) });
      setFallbackId(prev);
    }
  }

  async function changeAux(modelId: string) {
    const next = modelId === "" ? null : modelId;
    const prev = auxId;
    setAuxId(next);
    try {
      await setAuxModelId(next);
    } catch (err) {
      notify.error({ title: "辅助模型设置失败", message: String(err) });
      setAuxId(prev);
    }
  }

  return (
    <div className="settings-section-surface overflow-hidden rounded-lg border border-border bg-surface">
      <SettingItem
        title="备用模型（fallback）"
        description="主模型调用失败时一次性降级使用。可留空。"
        icon={Shuffle}
      >
        <Select
          className="text-sm h-10 w-full rounded-lg border border-border bg-background px-3 text-foreground outline-none transition focus:border-ring"
          value={fallbackId ?? ""}
          searchable
          searchPlaceholder="筛选备用模型"
          onChange={(value) => void changeFallback(value)}
          options={fallbackOptions}
          renderOption={(option) => (
            <span className="min-w-0">
              <span className="block truncate">{option.label}</span>
              {option.description && (
                <span className="mt-1 block truncate text-xs text-foreground-muted">
                  {option.description}
                </span>
              )}
            </span>
          )}
        />
      </SettingItem>
      <SettingItem
        title="辅助模型"
        description="标题归纳与快捷建议所用的模型。建议选便宜的小/非推理模型，省钱更快；默认跟随会话模型。"
        icon={Sparkles}
      >
        <Select
          className="text-sm h-10 w-full rounded-lg border border-border bg-background px-3 text-foreground outline-none transition focus:border-ring"
          value={auxId ?? ""}
          searchable
          searchPlaceholder="筛选辅助模型"
          onChange={(value) => void changeAux(value)}
          options={auxOptions}
          renderOption={(option) => (
            <span className="min-w-0">
              <span className="block truncate">{option.label}</span>
              {option.description && (
                <span className="mt-1 block truncate text-xs text-foreground-muted">
                  {option.description}
                </span>
              )}
            </span>
          )}
        />
      </SettingItem>
    </div>
  );
}

function ProviderDetailView({
  formatContextLimit,
  modelRows,
  modelsError,
  modelsLoading,
  onAddModel,
  onBack,
  onDeleteModel,
  onDeleteProvider,
  onRetryFetch,
  onSaveProvider,
  onSetContextLimit,
  onToggleVision,
  onSetProviderConfigOpen,
  onTestProvider,
  onToggleModel,
  onToggleProvider,
  preset,
  provider,
  providerConfigOpen,
  testingId,
}: {
  formatContextLimit: (limit: number | null | undefined) => string;
  modelRows: DetailModelRow[];
  modelsError: string | null;
  modelsLoading: boolean;
  onAddModel: (providerId: string) => Promise<void>;
  onBack: () => void;
  onDeleteModel: (model: ModelEntry) => Promise<void>;
  onDeleteProvider: (provider: Provider) => Promise<void>;
  onRetryFetch: (providerId: string) => Promise<void>;
  onSaveProvider: (input: ProviderInput) => Promise<void>;
  onSetContextLimit: (model: ModelEntry) => Promise<void>;
  onToggleVision: (model: ModelEntry) => Promise<void>;
  onSetProviderConfigOpen: Dispatch<SetStateAction<boolean>>;
  onTestProvider: (provider: Provider) => Promise<void>;
  onToggleModel: (providerId: string, row: DetailModelRow, enabled: boolean) => Promise<void>;
  onToggleProvider: (provider: Provider, enabled: boolean) => Promise<void>;
  preset: ProviderPreset | null;
  provider: Provider | null;
  providerConfigOpen: boolean;
  testingId: string | null;
}) {
  const title = provider?.name ?? preset?.name ?? "厂商详情";
  const baseUrl = provider?.baseUrl ?? preset?.baseUrl ?? "";
  const showCreateForm = provider === null;

  return (
    <section className="flex flex-col gap-5" aria-label="厂商详情">
      <div className="min-w-0">
        <button
          type="button"
          onClick={onBack}
          className="mb-3 inline-flex items-center gap-1 text-xs font-medium text-foreground-muted transition hover:text-foreground"
        >
          <ArrowLeft className="h-3.5 w-3.5" aria-hidden="true" />
          返回模型厂商
        </button>
      </div>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="truncate text-lg font-semibold text-foreground">{title}</h3>
          <p className="mt-1 truncate text-sm text-foreground-muted">{baseUrl || "配置自定义 OpenAI-compatible 厂商"}</p>
        </div>
        {provider && (
          <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
            <Badge tone={provider.hasSecret ? "success" : "neutral"}>
              {provider.hasSecret ? `密钥已配置${provider.secretHint ? ` · ${provider.secretHint}` : ""}` : "未配置密钥"}
            </Badge>
            {provider.lastCheck && (
              <Tooltip content={provider.lastCheck.detail}>
                <Badge tone={provider.lastCheck.status === "ready" ? "success" : "danger"}>
                  {provider.lastCheck.status === "ready" ? provider.lastCheck.detail : "连通失败"}
                </Badge>
              </Tooltip>
            )}
            <Switch checked={provider.enabled} onChange={(value) => void onToggleProvider(provider, value)} />
            <Button className="px-3 py-2" tone="outline" onClick={() => void onTestProvider(provider)} disabled={testingId === provider.id}>
              <Wifi className="h-4 w-4" /> {testingId === provider.id ? "测试中..." : "测试"}
            </Button>
            <Button className="px-3 py-2" tone="outline" onClick={() => onSetProviderConfigOpen(true)}>
              编辑
            </Button>
            <Tooltip content="删除厂商">
              <Button className="px-3 py-2" tone="outline" aria-label="删除厂商" onClick={() => void onDeleteProvider(provider)}>
                <Trash2 className="h-4 w-4" />
              </Button>
            </Tooltip>
          </div>
        )}
      </div>

      {showCreateForm ? (
        <div className="rounded-lg border border-border bg-surface px-5 py-4">
          <div className="text-sm font-semibold text-foreground">厂商配置</div>
          <p className="mt-1 text-xs text-foreground-muted">保存厂商名称、Base URL 和 API Key 后即可配置模型。</p>
          <ProviderForm
            key={preset?.key ?? "new-provider"}
            initial={null}
            initialValues={preset ? { name: preset.name, baseUrl: preset.baseUrl } : undefined}
            onSubmit={onSaveProvider}
          />
        </div>
      ) : (
        <>
          <div className="rounded-lg ">
            <div className="flex flex-wrap items-center justify-between gap-3 px-2 py-3">
              <div className="min-w-0 flex gap-2 items-end">
                <div className="text-sm font-semibold text-foreground">模型</div>
                <p className="mt-1 text-xs text-foreground-muted">
                  {modelsLoading ? "正在拉取厂商模型列表…" : `共 ${modelRows.length} 个模型，点击开关即可启用。`}
                </p>
              </div>
              <div className="flex shrink-0 flex-wrap justify-end gap-2">
                <Button className="px-3 py-1.5 text-xs" tone="outline" onClick={() => void onAddModel(provider.id)}>
                  <Plus className="h-3.5 w-3.5" /> 添加模型
                </Button>
              </div>
            </div>
            {modelsError && (
              <div className="flex flex-wrap items-start justify-between gap-3 border-t border-border px-5 py-3">
                <p className="min-w-0 flex-1 max-h-24 overflow-auto whitespace-pre-wrap break-words text-xs text-destructive">
                  拉取模型失败：{modelsError}
                </p>
                <Button className="shrink-0 px-3 py-1.5 text-xs" tone="outline" onClick={() => void onRetryFetch(provider.id)}>
                  重试
                </Button>
              </div>
            )}
            {modelsLoading && modelRows.length === 0 ? (
              <p className="border-t border-border px-5 py-5 text-sm text-foreground-muted">加载模型中…</p>
            ) : modelRows.length === 0 ? (
              <p className="border-t border-border px-5 py-5 text-sm text-foreground-muted">
                {modelsError ? "未拉取到模型，可点击「添加模型」手动添加。" : "暂无模型。"}
              </p>
            ) : (
              <ProviderModelTable
                formatContextLimit={formatContextLimit}
                modelRows={modelRows}
                onDeleteModel={onDeleteModel}
                onSetContextLimit={onSetContextLimit}
                onToggleVision={onToggleVision}
                onToggleModel={(row, enabled) => onToggleModel(provider.id, row, enabled)}
              />
            )}
          </div>

          <Drawer
            open={providerConfigOpen}
            onClose={() => onSetProviderConfigOpen(false)}
            title="编辑厂商配置"
            widthClassName="w-[min(620px,92vw)]"
          >
            <DrawerHeader onClose={() => onSetProviderConfigOpen(false)}>
              <div className="flex items-end gap-2">
                <h2 className="text-base font-semibold text-foreground">编辑厂商配置</h2>
                <p className="text-xs text-foreground-muted">更新厂商名称、Base URL 和 API Key。</p>
              </div>
            </DrawerHeader>
            <div className="min-h-0 overflow-auto px-5 py-4">
              <ProviderForm
                key={provider.id}
                initial={provider}
                onCancel={() => onSetProviderConfigOpen(false)}
                onSubmit={onSaveProvider}
              />
            </div>
          </Drawer>
        </>
      )}
    </section>
  );
}

function ProviderModelTable({
  formatContextLimit,
  modelRows,
  onDeleteModel,
  onSetContextLimit,
  onToggleVision,
  onToggleModel,
}: {
  formatContextLimit: (limit: number | null | undefined) => string;
  modelRows: DetailModelRow[];
  onDeleteModel: (model: ModelEntry) => Promise<void>;
  onSetContextLimit: (model: ModelEntry) => Promise<void>;
  onToggleVision: (model: ModelEntry) => Promise<void>;
  onToggleModel: (row: DetailModelRow, enabled: boolean) => Promise<void>;
}) {
  return (
    <div className="rounded-md border border-border bg-surface">
      <div className=" rounded-t-md grid grid-cols-[minmax(0,1fr)_72px_56px] items-center gap-3 bg-background px-5 py-2 text-xs font-medium text-foreground-muted sm:grid-cols-[minmax(0,1fr)_120px_72px] sm:gap-4">
        <span>模型名称</span>
        <span className="text-center">启用</span>
        <span className="text-right">操作</span>
      </div>
      {modelRows.map((row) => {
        const entry = row.entry;
        const multimodal = entry ? entry.supportsVision ?? entry.visionCapable : false;
        return (
          <div key={row.model} className="grid min-h-12 grid-cols-[minmax(0,1fr)_72px_56px] items-center gap-3 border-t border-border px-5 py-2 transition hover:bg-accent sm:grid-cols-[minmax(0,1fr)_120px_72px] sm:gap-4">
            <div className="flex min-w-0 flex-col gap-1.5">
              <div className="flex min-w-0 items-center gap-2">
                <span className="truncate text-sm text-foreground">{entry?.displayName || row.model}</span>
                {entry?.isDefault && <Badge tone="info">全局默认</Badge>}
              </div>
              {entry && (
                <div className="flex flex-wrap items-center gap-1.5">
                  <button
                    type="button"
                    className="group/ctx inline-flex items-center gap-1 rounded-full border border-border bg-background px-2.5 py-0.5 text-[11px] font-medium text-foreground-muted transition hover:border-ring hover:text-foreground"
                    onClick={() => void onSetContextLimit(entry)}
                    title="点击修改上下文上限"
                  >
                    <span>上下文 {formatContextLimit(entry.contextLimit)}</span>
                    <Pencil className="hidden h-3 w-3 group-hover/ctx:block" aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    className={
                      multimodal
                        ? "inline-flex items-center gap-1 rounded-full border border-success-border bg-success-subtle px-2.5 py-0.5 text-[11px] font-medium text-success transition"
                        : "inline-flex items-center gap-1 rounded-full border border-border bg-muted px-2.5 py-0.5 text-[11px] font-medium text-foreground-muted transition hover:text-foreground"
                    }
                    onClick={() => void onToggleVision(entry)}
                    title="点击切换：多模态 / 非多模态"
                  >
                    {multimodal ? <Eye className="h-3 w-3" aria-hidden="true" /> : <EyeOff className="h-3 w-3" aria-hidden="true" />}
                    {multimodal ? "多模态" : "非多模态"}
                  </button>
                </div>
              )}
            </div>
            <div className="flex justify-center">
              <Switch checked={row.enabled} onChange={(value) => void onToggleModel(row, value)} />
            </div>
            <div className="flex shrink-0 items-center justify-end">
              <button
                type="button"
                className="text-foreground-muted hover:text-destructive disabled:cursor-not-allowed disabled:opacity-40"
                disabled={!entry}
                aria-label="删除模型"
                onClick={() => entry && void onDeleteModel(entry)}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </div>
          </div>
        );
      })}
    </div>
  );
}
