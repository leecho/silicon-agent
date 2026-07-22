import { readFileSync } from "node:fs";

const pageSource = readFileSync("src/pages/mcp/McpPage.tsx", "utf8");
const apiSource = readFileSync("src/api.ts", "utf8");
const catalog = JSON.parse(readFileSync("src-tauri/builtin-mcp/catalog.json", "utf8")) as {
  presets: Array<{ presetId: string; auth: { type: string; valuePrefix?: string } }>;
};

for (const required of [
  "const authRequirement = editing ? getAuthRequirement(editing, editingPreset) : null;",
  "authRequirement?.kind === \"api_key\"",
  "必须填写授权信息",
  "formatApiKeyForSave(apiKey, authRequirement)",
  "authRequirement?.kind === \"oauth\"",
  "await mcpOauthAuthorize(saved.id);",
  "function hasTransportPlaceholders",
  "const unsupported = hasTransportPlaceholders(preset);",
  "该连接器还需要变量配置，当前版本暂不能直接连接。",
]) {
  if (!pageSource.includes(required)) {
    throw new Error(`MCP connection flow should enforce real auth setup: missing ${required}`);
  }
}

if (!apiSource.includes("value_prefix?: string | null")) {
  throw new Error("McpAuthConfig should carry ApiKey value_prefix to backend");
}

const github = catalog.presets.find((preset) => preset.presetId === "github");
if (github?.auth.type !== "api_key" || github.auth.valuePrefix !== "Bearer ") {
  throw new Error("github preset should declare Bearer API key prefix");
}
