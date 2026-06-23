import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/settings/sections/ProviderSection.tsx", "utf8");

if (!source.includes("全局默认模型")) {
  throw new Error("ProviderSection should render global default model in a dedicated section");
}

if (!source.includes("GlobalDefaultModelPanel")) {
  throw new Error("ProviderSection should isolate default model selection in GlobalDefaultModelPanel");
}

if (source.includes("默认模型</div>")) {
  throw new Error("Provider cards should not show default model as a provider stat");
}

if (source.includes("设为默认")) {
  throw new Error("Model rows should not expose per-provider set-default actions");
}

const panelStart = source.indexOf("function GlobalDefaultModelPanel");
const panelEnd = source.indexOf("function ProviderDetailView", panelStart);
const panelSource = source.slice(panelStart, panelEnd);

if (panelSource.includes("未设置默认模型")) {
  throw new Error("Global default model panel should not duplicate the selected model outside the selector");
}

if (panelSource.includes("{defaultModel.displayName || defaultModel.model}")) {
  throw new Error("Global default model panel should let the selector display the selected model");
}
