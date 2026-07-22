import { readFileSync } from "node:fs";

const catalog = JSON.parse(readFileSync("src-tauri/builtin-mcp/catalog.json", "utf8")) as {
  presets: Array<{
    presetId: string;
    displayName: string;
    descriptionZh: string;
  }>;
};

function preset(id: string) {
  const item = catalog.presets.find((p) => p.presetId === id);
  if (!item) throw new Error(`Missing connector preset: ${id}`);
  return item;
}

const expectedNames: Record<string, string> = {
  "baidu-netdisk": "百度网盘",
  cloudbase: "腾讯云开发",
  "edgeone-pages": "EdgeOne Pages",
  gmail: "Gmail",
  "tencent-docs": "腾讯文档",
  "tencent-weiyun": "微云",
};

for (const [id, expected] of Object.entries(expectedNames)) {
  const item = preset(id);
  if (item.displayName !== expected) {
    throw new Error(`${id} should display as ${expected}, got ${item.displayName}`);
  }
}

for (const id of Object.keys(expectedNames)) {
  const item = preset(id);
  if (item.descriptionZh.includes("MCP connector")) {
    throw new Error(`${id} should not expose fallback MCP connector copy`);
  }
  if (item.descriptionZh === item.presetId) {
    throw new Error(`${id} should not use its preset id as description`);
  }
}

for (const item of catalog.presets) {
  if (item.description.includes("MCP connector") || item.descriptionZh.includes("MCP connector")) {
    throw new Error(`${item.presetId} should not expose fallback MCP connector copy`);
  }
}
