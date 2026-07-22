import {
  buildMcpCatalogRows,
  type McpCatalogPreset,
  type McpCatalogServer,
  type McpCatalogStatus,
} from "../../../src/pages/mcp/mcpCatalog.ts";

function assertEqual<T>(actual: T, expected: T, message: string) {
  if (actual !== expected) {
    throw new Error(`${message}: expected ${String(expected)}, got ${String(actual)}`);
  }
}

const presets: McpCatalogPreset[] = [
  {
    presetId: "deepwiki",
    displayName: "DeepWiki",
    description: "Ask questions about public repositories",
    descriptionZh: "查询公开仓库",
    transport: { type: "http", url: "https://mcp.deepwiki.com/mcp" },
    auth: { type: "none" },
  },
  {
    presetId: "github",
    displayName: "GitHub",
    description: "GitHub official MCP",
    descriptionZh: "GitHub 官方 MCP",
    transport: { type: "http", url: "https://api.githubcopilot.com/mcp/" },
    auth: { type: "api_key", headerName: "Authorization" },
  },
];

const servers: McpCatalogServer[] = [
  {
    id: "mcp-1",
    name: "DeepWiki",
    presetId: "deepwiki",
    transport: { type: "http", url: "https://mcp.deepwiki.com/mcp" },
    auth: { type: "none" },
    autoApprove: false,
    enabled: true,
  },
  {
    id: "mcp-custom",
    name: "Local Docs",
    presetId: null,
    transport: { type: "stdio", command: "docs-mcp", args: [] },
    auth: { type: "none" },
    autoApprove: false,
    enabled: false,
  },
];

const statuses: Record<string, McpCatalogStatus> = {
  "mcp-1": {
    serverId: "mcp-1",
    state: "connected",
    error: null,
    toolCount: 4,
  },
};

const rows = buildMcpCatalogRows(presets, servers, statuses);

assertEqual(rows.length, 2, "catalog should render one row per preset");
assertEqual(rows[0]?.preset.presetId, "deepwiki", "first row should keep preset order");
assertEqual(rows[0]?.server?.id, "mcp-1", "configured preset should attach saved server");
assertEqual(rows[0]?.configured, true, "configured preset should be marked configured");
assertEqual(rows[0]?.status?.toolCount, 4, "configured preset should expose status");
assertEqual(rows[1]?.preset.presetId, "github", "second row should keep preset order");
assertEqual(rows[1]?.server, null, "unconfigured preset should not attach a server");
assertEqual(rows[1]?.configured, false, "unconfigured preset should be marked unconfigured");
