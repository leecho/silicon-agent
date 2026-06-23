import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/settings/sections/ProviderSection.tsx", "utf8");

if (!source.includes("Drawer") || !source.includes("DrawerHeader")) {
  throw new Error("ProviderSection should use Drawer for secondary provider/model configuration panels");
}

if (!source.includes('title="编辑厂商配置"')) {
  throw new Error("Provider config editing should open in a drawer titled 编辑厂商配置");
}

if (!source.includes('title="添加模型"')) {
  throw new Error("Adding models should open in a drawer titled 添加模型");
}

if (source.includes("providerConfigOpen && (")) {
  throw new Error("Provider config editing should not render as an inline expanded panel");
}

if (source.includes("addModelFor === provider.id && (")) {
  throw new Error("Adding models should not render as an inline expanded panel");
}
