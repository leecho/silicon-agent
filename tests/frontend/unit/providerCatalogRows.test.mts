import {
  buildProviderCatalogRows,
  type ProviderCatalogModel,
  type ProviderCatalogProvider,
} from "../../../src/pages/settings/sections/providerCatalog.ts";
import { PROVIDER_PRESETS } from "../../../src/pages/settings/sections/providerPresets.ts";

function assertEqual<T>(actual: T, expected: T, message: string) {
  if (actual !== expected) {
    throw new Error(`${message}: expected ${String(expected)}, got ${String(actual)}`);
  }
}

const providers: ProviderCatalogProvider[] = [
  {
    id: "provider-deepseek",
    name: "DeepSeek",
    baseUrl: "https://api.deepseek.com/v1/",
    hasSecret: true,
    secretHint: "sk-...1234",
    enabled: true,
    lastCheck: null,
    sortOrder: 0,
  },
  {
    id: "provider-custom",
    name: "Local Router",
    baseUrl: "http://localhost:11434/v1",
    hasSecret: false,
    secretHint: null,
    enabled: false,
    lastCheck: null,
    sortOrder: 1,
  },
];

const modelsByProvider: Record<string, ProviderCatalogModel[]> = {
  "provider-deepseek": [
    {
      id: "model-chat",
      providerId: "provider-deepseek",
      model: "deepseek-chat",
      displayName: null,
      enabled: true,
      isDefault: true,
      sortOrder: 0,
      contextLimit: null,
    },
    {
      id: "model-reasoner",
      providerId: "provider-deepseek",
      model: "deepseek-reasoner",
      displayName: null,
      enabled: false,
      isDefault: false,
      sortOrder: 1,
      contextLimit: null,
    },
  ],
  "provider-custom": [],
};

const rows = buildProviderCatalogRows(PROVIDER_PRESETS, providers, modelsByProvider);

const deepseek = rows.presetRows.find((row) => row.preset.key === "deepseek");
const openai = rows.presetRows.find((row) => row.preset.key === "openai");

assertEqual(rows.presetRows.length, PROVIDER_PRESETS.length - 1, "catalog should render every non-custom preset");
assertEqual(deepseek?.provider?.id, "provider-deepseek", "preset row should attach matching provider");
assertEqual(deepseek?.configured, true, "provider with saved secret should be configured");
assertEqual(deepseek?.modelCount, 2, "preset row should expose configured model count");
assertEqual(deepseek?.enabledModelCount, 1, "preset row should expose enabled model count");
assertEqual(openai?.provider, null, "unconfigured preset should still render without provider");
assertEqual(openai?.configured, false, "unconfigured preset should not be marked configured");
assertEqual(rows.customRows.length, 1, "custom providers should render in a separate section");
assertEqual(rows.customRows[0]?.provider.id, "provider-custom", "custom row should keep unmatched provider");
