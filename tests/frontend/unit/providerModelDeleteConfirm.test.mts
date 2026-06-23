import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/settings/sections/ProviderSection.tsx", "utf8");

if (!source.includes("async function removeModel")) {
  throw new Error("ProviderSection should keep model deletion in a dedicated removeModel function");
}

const removeModelStart = source.indexOf("async function removeModel");
const removeModelEnd = source.indexOf("function openProviderDrawer", removeModelStart);
const removeModelSource = source.slice(removeModelStart, removeModelEnd);

if (!removeModelSource.includes("message.confirm")) {
  throw new Error("Deleting a model should use MessageProvider confirm for second confirmation");
}

if (removeModelSource.includes("window.confirm")) {
  throw new Error("Deleting a model should not use native window.confirm");
}

if (!removeModelSource.includes('title: "删除模型"')) {
  throw new Error("The model delete confirmation should have a clear title");
}

if (!removeModelSource.includes('confirmText: "删除"')) {
  throw new Error("The model delete confirmation should use an explicit destructive confirm label");
}
