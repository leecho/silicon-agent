import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/mcp/McpPage.tsx", "utf8");

if (!source.includes("const toolList = tools[s.id] ?? [];")) {
  throw new Error("Expanded MCP rows should treat unloaded tool lists as an empty array");
}

if (source.includes("const toolList = tools[s.id];")) {
  throw new Error("Expanded MCP rows must not render from a possibly undefined tool list");
}
