import type { ModelEntry, Provider } from "../../../types";
import type { ProviderPreset } from "./providerPresets";

export type ProviderCatalogProvider = Provider;
export type ProviderCatalogModel = ModelEntry;

export interface ProviderCatalogRow {
  preset: ProviderPreset;
  provider: ProviderCatalogProvider | null;
  models: ProviderCatalogModel[];
  configured: boolean;
  modelCount: number;
  enabledModelCount: number;
}

export interface CustomProviderCatalogRow {
  provider: ProviderCatalogProvider;
  models: ProviderCatalogModel[];
  configured: boolean;
  modelCount: number;
  enabledModelCount: number;
}

export interface ProviderCatalogRows {
  presetRows: ProviderCatalogRow[];
  customRows: CustomProviderCatalogRow[];
}

function normalizeBaseUrl(value: string): string {
  return value.trim().replace(/\/+$/, "").toLowerCase();
}

function summarizeModels(models: ProviderCatalogModel[]) {
  return {
    modelCount: models.length,
    enabledModelCount: models.filter((model) => model.enabled).length,
  };
}

export function buildProviderCatalogRows(
  presets: ProviderPreset[],
  providers: ProviderCatalogProvider[],
  modelsByProvider: Record<string, ProviderCatalogModel[]>,
): ProviderCatalogRows {
  const nonCustomPresets = presets.filter((preset) => preset.key !== "custom");
  const providersByBaseUrl = new Map<string, ProviderCatalogProvider>();
  for (const provider of providers) {
    const key = normalizeBaseUrl(provider.baseUrl);
    if (key && !providersByBaseUrl.has(key)) {
      providersByBaseUrl.set(key, provider);
    }
  }

  const matchedProviderIds = new Set<string>();
  const presetRows = nonCustomPresets.map((preset) => {
    const provider = providersByBaseUrl.get(normalizeBaseUrl(preset.baseUrl)) ?? null;
    if (provider) matchedProviderIds.add(provider.id);
    const models = provider ? modelsByProvider[provider.id] ?? [] : [];
    return {
      preset,
      provider,
      models,
      configured: Boolean(provider?.hasSecret),
      ...summarizeModels(models),
    };
  });

  const customRows = providers
    .filter((provider) => !matchedProviderIds.has(provider.id))
    .map((provider) => {
      const models = modelsByProvider[provider.id] ?? [];
      return {
        provider,
        models,
        configured: Boolean(provider.hasSecret),
        ...summarizeModels(models),
      };
    });

  return { presetRows, customRows };
}
