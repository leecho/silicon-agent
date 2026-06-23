import { readFileSync } from "node:fs";

const appSource = readFileSync("src/App.tsx", "utf8");
const stylesSource = readFileSync("src/styles.css", "utf8");

if (appSource.includes("[&_.session-header]:pl-[204px]")) {
  throw new Error(
    "session header padding should not be toggled as a discrete Tailwind class",
  );
}

if (!appSource.includes("--session-header-padding-left")) {
  throw new Error(
    "AppShellContent should drive session header padding through a CSS variable",
  );
}

if (!appSource.includes("[&_.session-header]:transition-[padding-left]")) {
  throw new Error(
    "session header padding should transition with the sidebar grid animation",
  );
}

if (!appSource.includes("--session-body-padding-inline")) {
  throw new Error(
    "session feed inline padding should use the same CSS variable pattern",
  );
}

if (stylesSource.includes(".sidebar-collapsed .session-header")) {
  throw new Error(
    "legacy sidebar-collapsed header padding rule should be removed",
  );
}
