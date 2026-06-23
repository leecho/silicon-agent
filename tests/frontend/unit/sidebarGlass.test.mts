import { readFileSync } from "node:fs";

const appSource = readFileSync("src/App.tsx", "utf8");
const sidebarSource = readFileSync("src/components/layout/Sidebar.tsx", "utf8");
const stylesSource = readFileSync("src/styles.css", "utf8");

for (const forbidden of ["--sidebar-glass-bg", "--sidebar-glass-border", "--sidebar", "--shell"]) {
  if (stylesSource.includes(forbidden) || sidebarSource.includes(forbidden) || appSource.includes(forbidden)) {
    throw new Error(`Sidebar should not depend on region-specific token ${forbidden}`);
  }
}

if (!sidebarSource.includes("bg-card") && !sidebarSource.includes("bg-background")) {
  throw new Error("Sidebar should use core surface tokens");
}

if (!sidebarSource.includes("text-card-foreground") && !sidebarSource.includes("text-foreground")) {
  throw new Error("Sidebar should use core foreground tokens");
}

if (!sidebarSource.includes("border-border")) {
  throw new Error("Sidebar should use the core border token");
}

const appShellClass = appSource.match(/<main\s+[^>]*className="([^"]*h-screen[^"]*)"/s)?.[1] ?? "";
if (!appShellClass.includes("bg-background") || !appShellClass.includes("text-foreground")) {
  throw new Error("App shell should use background and foreground core tokens");
}
