import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const sourceRoots = ["src"];
const configFiles = ["tailwind.config.js"];
const ignoredDirs = new Set(["node_modules", ".git", ".superpowers", "dist", "target"]);

function listFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      return ignoredDirs.has(entry) ? [] : listFiles(path);
    }
    return /\.(css|ts|tsx|js|jsx|mts)$/.test(path) ? [path] : [];
  });
}

const checkedFiles = [...sourceRoots.flatMap(listFiles), ...configFiles];

const forbiddenPatterns = [
  /--sc-/,
  /--sidebar-/,
  /--shell-/,
  /\bapp-(page-title|page-description|section-title|section-description|field-description|body-text|field-title|caption|control-text)\b/,
  /\bui-control-surface\b/,
  /font-\[var\(--app-font-family\)\]/,
  /bg-bg-/,
  /bg-text-/,
  /text-text-/,
  /text-bg-/,
  /border-border-focus/,
  /(?:bg|text|border|focus:border|hover:text|hover:bg)-action-/,
  /(?:bg|text|border)-status-(success|warning|info|danger)-/,
  /(?:session|composer|agent|plugin|skill|memory)-[a-z-]+:/
];

const allowedCssTokens = [
  "--background",
  "--foreground",
  "--foreground-secondary",
  "--foreground-muted",
  "--surface",
  "--card",
  "--card-foreground",
  "--popover",
  "--popover-foreground",
  "--primary",
  "--primary-rgb",
  "--primary-foreground",
  "--secondary",
  "--secondary-foreground",
  "--muted",
  "--muted-foreground",
  "--accent",
  "--accent-foreground",
  "--success",
  "--success-rgb",
  "--success-foreground",
  "--success-subtle",
  "--success-border",
  "--warning",
  "--warning-rgb",
  "--warning-foreground",
  "--warning-subtle",
  "--warning-border",
  "--danger",
  "--danger-rgb",
  "--danger-foreground",
  "--danger-subtle",
  "--danger-border",
  "--destructive",
  "--destructive-rgb",
  "--destructive-foreground",
  "--border-subtle",
  "--border",
  "--border-strong",
  "--input",
  "--ring",
  "--ring-rgb",
  "--radius"
];

const offenders = checkedFiles.flatMap((file) => {
  const source = readFileSync(file, "utf8");
  return forbiddenPatterns
    .filter((pattern) => pattern.test(source))
    .map((pattern) => `${file} matches ${pattern}`);
});

if (offenders.length > 0) {
  throw new Error(`Forbidden UI token language remains:\n${offenders.join("\n")}`);
}

const styles = readFileSync("src/styles.css", "utf8");
const escapeRegExp = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const missingTokens = allowedCssTokens.filter((token) => !new RegExp(`${escapeRegExp(token)}\\s*:`).test(styles));
if (missingTokens.length > 0) {
  throw new Error(`styles.css is missing core UI tokens: ${missingTokens.join(", ")}`);
}

const tailwind = readFileSync("tailwind.config.js", "utf8");
const isJsIdentifier = (value: string) => /^[A-Za-z_$][\w$]*$/.test(value);
const alphaColorUtilities = new Set(["primary", "success", "warning", "danger", "destructive", "ring"]);
for (const utility of [
  "background",
  "foreground",
  "foreground-secondary",
  "foreground-muted",
  "surface",
  "card",
  "card-foreground",
  "popover",
  "popover-foreground",
  "primary",
  "primary-foreground",
  "secondary",
  "secondary-foreground",
  "muted",
  "muted-foreground",
  "accent",
  "accent-foreground",
  "success",
  "success-foreground",
  "success-subtle",
  "success-border",
  "warning",
  "warning-foreground",
  "warning-subtle",
  "warning-border",
  "danger",
  "danger-foreground",
  "danger-subtle",
  "danger-border",
  "destructive",
  "destructive-foreground",
  "border-subtle",
  "border",
  "border-strong",
  "input",
  "ring"
]) {
  const escapedUtility = escapeRegExp(utility);
  const keyPattern = isJsIdentifier(utility)
    ? `(?:^|[^A-Za-z0-9_$])(?:"${escapedUtility}"|${escapedUtility})`
    : `"${escapedUtility}"`;
  const expectedValue = alphaColorUtilities.has(utility)
    ? `rgb\\(var\\(--${escapedUtility}-rgb\\) / <alpha-value>\\)`
    : `var\\(--${escapedUtility}\\)`;
  const mappingPattern = new RegExp(`${keyPattern}\\s*:\\s*"${expectedValue}"`);
  if (!mappingPattern.test(tailwind)) {
    throw new Error(`tailwind.config.js should expose ${utility} as ${expectedValue}`);
  }
}
