import {
  hasEnabledModels,
  type EnabledModelGroup,
} from "../../../src/lib/modelAvailability.ts";

function assertEqual<T>(actual: T, expected: T, message: string) {
  if (actual !== expected) {
    throw new Error(`${message}: expected ${String(expected)}, got ${String(actual)}`);
  }
}

const emptyGroups: EnabledModelGroup[] = [];
assertEqual(hasEnabledModels(emptyGroups), false, "empty model groups are unavailable");

const emptyProviderModels: EnabledModelGroup[] = [
  { providerId: "provider_a", providerName: "Provider A", models: [] },
];
assertEqual(
  hasEnabledModels(emptyProviderModels),
  false,
  "providers without enabled models are unavailable",
);

const availableModels: EnabledModelGroup[] = [
  {
    providerId: "provider_a",
    providerName: "Provider A",
    models: [
      {
        id: "model_a",
        providerId: "provider_a",
        model: "gpt-compatible",
        displayName: null,
        enabled: true,
        isDefault: true,
        sortOrder: 0,
      },
    ],
  },
];
assertEqual(hasEnabledModels(availableModels), true, "one enabled model is available");
