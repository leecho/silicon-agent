import { useCallback, useEffect, useMemo, useState } from "react";
import { Boxes, Sparkles } from "lucide-react";
import {
  kbVectorSettings,
  kbSetVectorSettings,
  listProviderModels,
  listProviders,
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

/** 知识库设置：智能查找（向量检索）开关与向量模型。 */
export function KnowledgeBaseSection() {
  const notify = useNotifications();
  const [providers, setProviders] = useState<Provider[]>([]);
  const [modelsByProvider, setModelsByProvider] = useState<Record<string, ModelEntry[]>>({});
  const [kbVectorOn, setKbVectorOn] = useState(false);
  const [kbEmbeddingModel, setKbEmbeddingModel] = useState<string>("");

  const reload = useCallback(async () => {
    const ps = await listProviders();
    const entries = await Promise.all(
      ps.map(async (provider) => [provider.id, await listProviderModels(provider.id)] as const),
    );
    setProviders(ps);
    setModelsByProvider(Object.fromEntries(entries));
    try {
      const [vecOn, vecModel] = await kbVectorSettings();
      setKbVectorOn(vecOn);
      setKbEmbeddingModel(vecModel);
    } catch {
      /* 拿不到设置则用默认（关） */
    }
  }, []);

  useEffect(() => {
    reload().catch((err) => notify.error({ title: "加载失败", message: String(err) }));
  }, [reload, notify]);

  const enabledModelOptions = useMemo(
    () => buildEnabledModelOptions(providers, modelsByProvider),
    [providers, modelsByProvider],
  );

  async function persistVectorSettings(enabled: boolean, modelId: string) {
    setKbVectorOn(enabled);
    setKbEmbeddingModel(modelId);
    try {
      await kbSetVectorSettings(enabled, modelId);
    } catch (err) {
      notify.error({ title: "保存失败", message: String(err) });
    }
  }

  return (
    <section className="grid gap-8" aria-label="知识库设置">
      <div className="settings-section-surface overflow-hidden rounded-lg border border-border bg-surface">
        <SettingItem
          title="智能查找（向量检索）"
          description="开启后助手查阅资料库时结合语义相似度，更能找到换了说法的内容。需选一个向量（embedding）模型，并在各资料库里点「建立向量索引」。"
          icon={Sparkles}
        >
          <Switch
            checked={kbVectorOn}
            onChange={(v) => void persistVectorSettings(v, kbEmbeddingModel)}
          />
        </SettingItem>
        {kbVectorOn && (
          <SettingItem
            title="向量模型"
            description="用于把资料库内容与查询转成语义向量的 embedding 模型。建议选专用 embedding 模型。"
            icon={Boxes}
          >
            <Select
              className="text-sm h-10 w-full rounded-lg border border-border bg-background px-3 text-foreground outline-none transition focus:border-ring"
              value={kbEmbeddingModel}
              onChange={(id) => void persistVectorSettings(true, id)}
              options={enabledModelOptions}
              searchable
              searchPlaceholder="筛选向量模型"
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
        )}
      </div>
    </section>
  );
}
