import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/settings/sections/ProviderSection.tsx", "utf8");

if (!source.includes("useMessages")) {
  throw new Error("ProviderSection should use MessageProvider prompts for model context settings");
}

if (!source.includes("message.prompt")) {
  throw new Error("Setting a model context limit should open a MessageProvider prompt");
}

if (!source.includes("设置上下文")) {
  throw new Error("Model rows should expose a settings button for context limits");
}

if (!source.includes("上下文：")) {
  throw new Error("Model rows should display the current context limit");
}

if (source.includes('type="number"') && source.includes("saveContextLimit(m, e.target.value)")) {
  throw new Error("Model context limits should not be edited inline through a row input");
}
