import type { EnabledProviderModels } from "../types";

export type EnabledModelGroup = Pick<
  EnabledProviderModels,
  "providerId" | "providerName" | "models"
>;

export function hasEnabledModels(groups: EnabledModelGroup[]): boolean {
  return groups.some((group) => group.models.length > 0);
}
