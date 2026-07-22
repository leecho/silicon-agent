import { readFileSync } from "node:fs";

const appSource = readFileSync("src/App.tsx", "utf8");
const navSource = readFileSync("src/appNavigation.ts", "utf8");

if (!navSource.includes('"connectors"')) {
  throw new Error("Home navigation should expose a connectors section");
}

if (!navSource.includes('label: "连接器"')) {
  throw new Error("Connectors navigation item should use the 连接器 module name");
}

if (!appSource.includes('import { McpPage } from "./pages/mcp/McpPage";')) {
  throw new Error("McpPage should be imported as a first-level home page");
}

if (!appSource.includes("connectors: <McpPage />")) {
  throw new Error("Connectors section should render McpPage in the home content area");
}
