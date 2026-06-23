import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/settings/sections/CallLogSection.tsx", "utf8");

for (const token of [
  'from "../../../components/ui"',
  'from "../../../components/settings/SettingsControls"',
  'import { ArrowRight } from "lucide-react"',
  "Button",
  "Badge",
  "Drawer",
  "DrawerHeader",
  "Select",
  "Switch",
  "Tabs",
  "TextInput",
]) {
  if (!source.includes(token)) {
    throw new Error(`CallLogSection should use project UI primitives: missing ${token}`);
  }
}

for (const token of [
  'useSession()',
  "openSession(detail.sessionId)",
  "打开会话",
  "<ArrowRight",
]) {
  if (!source.includes(token)) {
    throw new Error(`Call log details should route to the owning session: missing ${token}`);
  }
}

for (const label of ["Input", "Output"]) {
  if (!source.includes(label)) {
    throw new Error(`Call log detail drawer should expose a ${label} tab`);
  }
}

for (const removed of ["MetricCard", "InfoTile", "renderPreview"]) {
  if (source.includes(removed)) {
    throw new Error(`CallLogSection should keep the original layout and remove ${removed}`);
  }
}

for (const token of [
  '<div className="flex flex-wrap items-start justify-between gap-3">',
  '<div className="flex flex-wrap gap-2">',
  "<table",
  'widthClassName="w-[640px] max-w-[90vw]"',
  'type DetailTab = "input" | "output";',
]) {
  if (!source.includes(token)) {
    throw new Error(`CallLogSection should keep the original layout: missing ${token}`);
  }
}

for (const removedTab of ['label: "Thinking"', 'label: "Tools"']) {
  if (source.includes(removedTab)) {
    throw new Error(`Detail drawer should only add Input/Output tabs, found ${removedTab}`);
  }
}

const openSessionIndex = source.indexOf("打开会话");
const arrowRightIndex = source.indexOf("<ArrowRight", openSessionIndex);
if (openSessionIndex === -1 || arrowRightIndex === -1 || arrowRightIndex < openSessionIndex) {
  throw new Error("Open session action should render the Lucide ArrowRight icon after the text");
}

for (const token of [
  'className="flex h-full min-h-0 flex-col overflow-hidden p-5"',
  'className="mt-4 flex min-h-0 flex-1 flex-col overflow-hidden"',
  'className="flex min-h-0 flex-1 flex-col overflow-hidden py-1"',
  'className="flex min-h-0 flex-1 border border-border rounded-md px-1 flex-col overflow-hidden"',
  'className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-surface p-3 text-xs leading-5 text-foreground-secondary"',
  "PayloadBlock",
  "formatPayload",
]) {
  if (!source.includes(token)) {
    throw new Error(`Input/output payload area should fill available drawer space and format content: missing ${token}`);
  }
}

for (const removed of [
  'className="mt-4 flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border-subtle bg-card"',
  "max-h-80",
  "<details",
  "<summary",
]) {
  if (source.includes(removed)) {
    throw new Error(`Payload presentation should use full-height panels, not the old compact disclosure: found ${removed}`);
  }
}

console.log("call log section UI assertions passed");
