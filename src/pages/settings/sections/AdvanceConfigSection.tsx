import { useCallback, useEffect, useMemo, useState } from "react";
import {
  GitBranch,
  Lightbulb,
  RefreshCcw,
  Scissors,
  Shuffle,
  Sparkles,
} from "lucide-react";
import {
  getAutoCompactEnabled,
  getAutoCompactThresholdPct,
  getAutoRetryMax,
  getAuxModelId,
  getFallbackModel,
  getMaxIterations,
  getSuggestionsEnabled,
  getSubagentExecutionMode,
  listProviderModels,
  listProviders,
  setAutoCompactEnabled,
  setAutoCompactThresholdPct,
  setAutoRetryMax,
  setAuxModelId,
  setFallbackModel,
  setMaxIterations,
  setSuggestionsEnabled,
  setSubagentExecutionMode,
  type SubagentExecutionMode,
} from "../../../api";
import type { ModelEntry, Provider } from "../../../types";
import { Select, Switch, useNotifications } from "../../../components/ui";
import { SettingItem } from "../../../components/settings/SettingsControls";

type ModelOption = {
  description?: string;
  group: string;
  label: string;
  searchText: string;
  value: string;
};

const RETRY_OPTIONS = [
  { label: "关闭", value: "0" },
  { label: "1 次", value: "1" },
  { label: "2 次", value: "2" },
  { label: "3 次", value: "3" },
  { label: "4 次", value: "4" },
  { label: "5 次", value: "5" },
];

const MAX_ITERATION_OPTIONS = [
  { label: "8 次", value: "8" },
  { label: "12 次", value: "12" },
  { label: "16 次", value: "16" },
  { label: "24 次", value: "24" },
  { label: "32 次", value: "32" },
  { label: "48 次", value: "48" },
  { label: "64 次", value: "64" },
];

const COMPACT_THRESHOLD_OPTIONS = [
  { label: "70%", value: "70" },
  { label: "75%", value: "75" },
  { label: "80%", value: "80" },
  { label: "85%", value: "85" },
  { label: "90%", value: "90" },
  { label: "95%", value: "95" },
];

const SUBAGENT_EXECUTION_MODE_OPTIONS: Array<{
  label: string;
  value: SubagentExecutionMode;
}> = [
  { label: "并行", value: "parallel" },
  { label: "串行", value: "serial" },
];

function buildEnabledModelOptions(
  providers: Provider[],
  modelsByProvider: Record<string, ModelEntry[]>,
): ModelOption[] {
  const providerNameById = new Map(
    providers.map((provider) => [provider.id, provider.name]),
  );
  return Object.values(modelsByProvider)
    .flat()
    .filter((model) => model.enabled)
    .map((model) => {
      const providerName = providerNameById.get(model.providerId) ?? "未命名厂商";
      const label = model.displayName || model.model;
      return {
        description: model.displayName ? model.model : undefined,
        group: providerName,
        label,
        searchText: [providerName, model.model, model.displayName]
          .filter((text): text is string => Boolean(text))
          .join(" "),
        value: model.id,
      };
    });
}

/** 模型通用配置：集中管理模型运行行为、fallback 和辅助模型。 */
export function AdvanceConfigSection() {
  const notify = useNotifications();
  const [providers, setProviders] = useState<Provider[]>([]);
  const [modelsByProvider, setModelsByProvider] = useState<Record<string, ModelEntry[]>>({});
  const [suggestionsOn, setSuggestionsOn] = useState(true);
  const [autoCompactOn, setAutoCompactOn] = useState(true);
  const [autoCompactThresholdPct, setAutoCompactThresholdPctState] = useState(90);
  const [autoRetryMax, setAutoRetryMaxState] = useState(3);
  const [maxIterations, setMaxIterationsState] = useState(24);
  const [subagentExecutionMode, setSubagentExecutionModeState] =
    useState<SubagentExecutionMode>("parallel");
  const [fallbackId, setFallbackId] = useState<string | null>(null);
  const [auxId, setAuxId] = useState<string | null>(null);

  const reload = useCallback(async () => {
    const ps = await listProviders();
    const entries = await Promise.all(
      ps.map(async (provider) => [provider.id, await listProviderModels(provider.id)] as const),
    );
    const [
      suggestionsEnabled,
      autoCompactEnabled,
      compactThresholdPct,
      retryMax,
      maxIterationCount,
      subagentMode,
      fallbackModelId,
      auxModelId,
    ] = await Promise.all([
      getSuggestionsEnabled(),
      getAutoCompactEnabled(),
      getAutoCompactThresholdPct(),
      getAutoRetryMax(),
      getMaxIterations(),
      getSubagentExecutionMode(),
      getFallbackModel(),
      getAuxModelId(),
    ]);
    setProviders(ps);
    setModelsByProvider(Object.fromEntries(entries));
    setSuggestionsOn(suggestionsEnabled);
    setAutoCompactOn(autoCompactEnabled);
    setAutoCompactThresholdPctState(compactThresholdPct);
    setAutoRetryMaxState(retryMax);
    setMaxIterationsState(maxIterationCount);
    setSubagentExecutionModeState(subagentMode);
    setFallbackId(fallbackModelId);
    setAuxId(auxModelId);
  }, []);

  useEffect(() => {
    reload().catch((err) => notify.error({ title: "加载失败", message: String(err) }));
  }, [reload, notify]);

  const enabledModelOptions = useMemo(
    () => buildEnabledModelOptions(providers, modelsByProvider),
    [providers, modelsByProvider],
  );
  const fallbackOptions = useMemo(
    () => [{ label: "不使用备用模型", value: "" }, ...enabledModelOptions],
    [enabledModelOptions],
  );
  const auxOptions = useMemo(
    () => [{ label: "跟随会话模型（默认）", value: "" }, ...enabledModelOptions],
    [enabledModelOptions],
  );

  async function toggleSuggestions(value: boolean) {
    setSuggestionsOn(value);
    try {
      await setSuggestionsEnabled(value);
    } catch (err) {
      notify.error({ title: "快捷建议设置失败", message: String(err) });
      setSuggestionsOn(!value);
    }
  }

  async function toggleAutoCompact(value: boolean) {
    setAutoCompactOn(value);
    try {
      await setAutoCompactEnabled(value);
    } catch (err) {
      notify.error({ title: "自动压缩设置失败", message: String(err) });
      setAutoCompactOn(!value);
    }
  }

  async function changeAutoRetry(value: string) {
    const next = Number(value);
    setAutoRetryMaxState(next);
    try {
      await setAutoRetryMax(next);
    } catch (err) {
      notify.error({ title: "自动重试设置失败", message: String(err) });
      await reload();
    }
  }

  async function changeAutoCompactThreshold(value: string) {
    const next = Number(value);
    setAutoCompactThresholdPctState(next);
    try {
      await setAutoCompactThresholdPct(next);
    } catch (err) {
      notify.error({ title: "上下文压缩阈值设置失败", message: String(err) });
      await reload();
    }
  }

  async function changeMaxIterations(value: string) {
    const next = Number(value);
    setMaxIterationsState(next);
    try {
      await setMaxIterations(next);
    } catch (err) {
      notify.error({ title: "最大迭代次数设置失败", message: String(err) });
      await reload();
    }
  }

  async function changeSubagentExecutionMode(value: string) {
    const next = value as SubagentExecutionMode;
    setSubagentExecutionModeState(next);
    try {
      await setSubagentExecutionMode(next);
    } catch (err) {
      notify.error({ title: "子代理执行方式设置失败", message: String(err) });
      await reload();
    }
  }

  async function changeFallback(modelId: string) {
    const next = modelId === "" ? null : modelId;
    setFallbackId(next);
    try {
      await setFallbackModel(next);
    } catch (err) {
      notify.error({ title: "备用模型设置失败", message: String(err) });
      await reload();
    }
  }

  async function changeAux(modelId: string) {
    const next = modelId === "" ? null : modelId;
    setAuxId(next);
    try {
      await setAuxModelId(next);
    } catch (err) {
      notify.error({ title: "辅助模型设置失败", message: String(err) });
      await reload();
    }
  }

  return (
    <section className="grid gap-8" aria-label="模型通用配置">
      <div className="settings-section-surface overflow-hidden rounded-lg border border-border bg-surface">
        <SettingItem
          title="快捷建议"
          description="每轮结束后用大模型生成「下一步」建议，点击可填入输入框。关闭可省一次模型调用。"
          icon={Lightbulb}
        >
          <Switch checked={suggestionsOn} onChange={(value) => void toggleSuggestions(value)} />
        </SettingItem>
        <SettingItem
          title="自动压缩上下文"
          description="上下文占用接近模型上限时，自动把较早历史压缩成摘要，避免超限。关闭后需手动 /compact。"
          icon={Scissors}
        >
          <Switch checked={autoCompactOn} onChange={(value) => void toggleAutoCompact(value)} />
        </SettingItem>
        {autoCompactOn && (<SettingItem
          title="上下文压缩阈值"
          description="自动压缩开启时，本轮 prompt 用量达到模型上下文上限的该比例后压缩较早历史。"
          icon={Scissors}
        >
          <Select
            className="text-sm h-10 w-28 rounded-lg border border-border bg-background px-3 text-foreground outline-none transition focus:border-ring"
            value={String(autoCompactThresholdPct)}
            tooltip="上下文压缩阈值"
            options={COMPACT_THRESHOLD_OPTIONS}
            onChange={(value) => void changeAutoCompactThreshold(value)}
          />
        </SettingItem>)}
        
        <SettingItem
          title="最大迭代次数"
          description="单次任务最多允许的模型-工具循环次数，达到上限后会停止继续迭代。"
          icon={RefreshCcw}
        >
          <Select
            className="text-sm h-10 w-28 rounded-lg border border-border bg-background px-3 text-foreground outline-none transition focus:border-ring"
            value={String(maxIterations)}
            tooltip="最大迭代次数"
            options={MAX_ITERATION_OPTIONS}
            onChange={(value) => void changeMaxIterations(value)}
          />
        </SettingItem>
        <SettingItem
          title="子代理执行方式"
          description="并行会同时启动同一轮派发的多个子代理；串行会按创建顺序逐个运行，适合资源紧张或任务有隐含先后关系时使用。"
          icon={GitBranch}
        >
          <Select
            className="text-sm h-10 w-28 rounded-lg border border-border bg-background px-3 text-foreground outline-none transition focus:border-ring"
            value={subagentExecutionMode}
            tooltip="子代理执行方式"
            options={SUBAGENT_EXECUTION_MODE_OPTIONS}
            onChange={(value) => void changeSubagentExecutionMode(value)}
          />
        </SettingItem>
        <SettingItem
          title="失败自动重试次数"
          description="模型调用遇到限流、服务端或网络等瞬时错误时自动重试的最大次数。0 表示关闭。"
          icon={RefreshCcw}
        >
          <Select
            className="text-sm h-10 w-28 rounded-lg border border-border bg-background px-3 text-foreground outline-none transition focus:border-ring"
            value={String(autoRetryMax)}
            tooltip="失败自动重试次数"
            options={RETRY_OPTIONS}
            onChange={(value) => void changeAutoRetry(value)}
          />
        </SettingItem>
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
    </section>
  );
}
