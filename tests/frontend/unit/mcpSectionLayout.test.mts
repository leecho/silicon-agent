import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/mcp/McpPage.tsx", "utf8");

if (source.includes('{/* 预置目录 */}\n      <div className="rounded-lg border border-border bg-card p-4">')) {
  throw new Error("Preset MCP section should not wrap rows in an outer card");
}

if (source.includes(">详情<")) {
  throw new Error("MCP row action should be labeled 配置 instead of 详情");
}

if (!source.includes("title={editingTitle}")) {
  throw new Error("Connector modal should use a user-facing dynamic title");
}

if (!source.includes("`连接 ${editing.name || \"连接器\"}`")) {
  throw new Error("Preset connector modal should present unconfigured presets as connecting");
}

if (!source.includes("lg:grid-cols-3")) {
  throw new Error("Preset connectors should use a three-card row layout on large screens");
}

if (!source.includes("group-hover:opacity-100")) {
  throw new Error("Connector action button should be revealed on card hover");
}

if (!source.includes('<div className="mb-5 mt-4 flex items-start justify-between gap-4">')) {
  throw new Error("Connector page should put primary actions in the title bar");
}

if (source.includes("{/* 顶部操作 */}")) {
  throw new Error("Connector page should not render a separate top action row");
}

for (const required of [
  "const [connectorQuery, setConnectorQuery] = useState(\"\");",
  "const filteredCatalogRows = useMemo(",
  "placeholder=\"搜索连接器名称或能力\"",
  "onChange={(e) => setConnectorQuery(e.target.value)}",
  "filteredCatalogRows.length === 0",
  "没有匹配的连接器",
]) {
  if (!source.includes(required)) {
    throw new Error(`Connector page should support searching marketplace connectors: missing ${required}`);
  }
}

for (const required of [
  "max-w-[860px]",
  "grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3",
  "group flex flex-col rounded-xl border border-border-subtle bg-surface p-4 transition hover:border-border",
  "pointer-events-none inline-flex shrink-0 items-center gap-1 rounded-md",
  "group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100",
  "line-clamp-2 text-xs leading-5 text-foreground-secondary",
]) {
  if (!source.includes(required)) {
    throw new Error(`Connector cards should follow the agent card layout: missing ${required}`);
  }
}

for (const staleLayout of ["max-w-[1280px]", "min-h-[136px]", "absolute right-4 top-4", "pr-16"]) {
  if (source.includes(staleLayout)) {
    throw new Error(`Connector cards should not use the sparse/overlapping layout: ${staleLayout}`);
  }
}

for (const technicalLabel of ["{transportLabel(preset)}", "{presetAuthLabel(preset)}", "{configLabel}"]) {
  if (source.includes(technicalLabel)) {
    throw new Error(`Preset connector cards should not expose technical label ${technicalLabel}`);
  }
}

for (const required of [
  "const isPresetEditing = Boolean(editing?.presetId);",
  "const isConnectedPresetEditing = Boolean(editing?.presetId && editing.id);",
  "const editingPreset =",
  "editingPreset?.descriptionZh || editingPreset?.description",
  "isConnectedPresetEditing ? \"保存\" : \"连接\"",
  "isConnectedPresetEditing ? \"保存中…\" : \"连接中…\"",
]) {
  if (!source.includes(required)) {
    throw new Error(`Preset connector modal should be connection-oriented: missing ${required}`);
  }
}

for (const required of [
  "已连接",
  "断开连接",
  "setPendingDelete(editing)",
  "closeEdit();",
  "checked={editing.enabled}",
  "onChange={(enabled) => setEditing({ ...editing, enabled })}",
]) {
  if (!source.includes(required)) {
    throw new Error(`Connected preset modal should expose management controls: missing ${required}`);
  }
}

for (const required of [
  "preset.iconPath ?",
  '<img src={preset.iconPath}',
  "{!preset.iconPath && initial}",
  "editingPreset?.iconPath ?",
]) {
  if (!source.includes(required)) {
    throw new Error(`Connector UI should render marketplace icons with a fallback: missing ${required}`);
  }
}

for (const implementationLeak of [
  "连接参数来自预置",
  "转为自定义",
]) {
  if (source.includes(implementationLeak)) {
    throw new Error(`Preset connector modal should hide implementation detail: ${implementationLeak}`);
  }
}
