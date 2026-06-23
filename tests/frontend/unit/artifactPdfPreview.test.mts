import { readFileSync } from "node:fs";

const apiSource = readFileSync("src/api.ts", "utf8");
const drawerSource = readFileSync("src/components/session/ArtifactPreviewDrawer.tsx", "utf8");

if (!apiSource.includes('kind: "markdown" | "text" | "pdf" | "html" | "office" | "binary";')) {
  throw new Error("ArtifactContent should include pdf, html, and office kinds");
}

if (!drawerSource.includes('state.content.kind === "pdf"')) {
  throw new Error("ArtifactPreviewDrawer should branch on pdf content");
}

if (!drawerSource.includes("<iframe")) {
  throw new Error("PDF artifacts should render in an iframe");
}

if (!drawerSource.includes("data-pdf-preview")) {
  throw new Error("PDF iframe should expose a stable preview marker");
}

if (!drawerSource.includes("src={state.content.content}")) {
  throw new Error("PDF iframe should use the artifact data URL");
}

if (!drawerSource.includes('className="h-full w-full bg-background"')) {
  throw new Error("PDF iframe should fill the preview area and use theme background");
}

if (!drawerSource.includes("bg-card") || !drawerSource.includes("border-border-subtle")) {
  throw new Error("PDF preview chrome should use semantic theme tokens");
}

if (!drawerSource.includes('state.content.kind === "html"')) {
  throw new Error("ArtifactPreviewDrawer should branch on html content");
}

if (!drawerSource.includes('state.content.kind === "office"')) {
  throw new Error("ArtifactPreviewDrawer should branch on office content");
}

if (!drawerSource.includes("data-office-preview")) {
  throw new Error("Office iframe should expose a stable preview marker");
}

if (!drawerSource.includes("srcDoc={withThemedStaticPreviewCsp(state.content.content, previewThemeCss)}")) {
  throw new Error("Office preview should render sanitized static HTML through themed CSP wrapper");
}

for (const required of [
  "readPreviewThemeCss",
  "--preview-bg-page",
  "--preview-bg-panel",
  "--preview-text-primary",
  "--preview-text-secondary",
  "document.documentElement.classList.contains(\"theme-dark\")",
  "MutationObserver",
  "data-app-preview-theme",
]) {
  if (!drawerSource.includes(required)) {
    throw new Error(`Office preview should follow app theme token: ${required}`);
  }
}

if (!drawerSource.includes("data-html-preview")) {
  throw new Error("HTML iframe should expose a stable preview marker");
}

for (const required of [
  "withHtmlNetworkPreviewCsp",
  'sandbox="allow-scripts"',
  "srcDoc={withHtmlNetworkPreviewCsp(state.content.content)}",
  'className="min-h-0 flex-1 bg-background"',
  'className="flex h-full min-h-0 flex-col overflow-hidden rounded-lg border border-border-subtle bg-card"',
  "script-src 'unsafe-inline' https:",
  "style-src 'unsafe-inline' https:",
  "img-src data: blob: https:",
  "font-src data: https:",
  "connect-src https:",
  "frame-src 'none'",
  "object-src 'none'",
  "form-action 'none'"
]) {
  if (!drawerSource.includes(required)) {
    throw new Error(`HTML interactive preview should include ${required}`);
  }
}

for (const forbidden of [
  "allow-same-origin",
  "allow-forms",
  "allow-popups",
  "allow-downloads",
  "allow-top-navigation"
]) {
  if (drawerSource.includes(forbidden)) {
    throw new Error(`HTML network preview should not grant ${forbidden}`);
  }
}
