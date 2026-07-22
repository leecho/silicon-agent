import { readFileSync } from "node:fs";

const dropdownSource = readFileSync("src/components/ui/DropdownMenu.tsx", "utf8");
const uiIndexSource = readFileSync("src/components/ui/index.ts", "utf8");
const taskCardSource = readFileSync("src/pages/scheduling/TaskCard.tsx", "utf8");
const permissionPickerSource = readFileSync(
  "src/pages/session/composer/PermissionPicker.tsx",
  "utf8",
);
const dropdownUsageSources = [
  "src/pages/session/composer/AddMenu.tsx",
  "src/pages/session/composer/ModelPicker.tsx",
  "src/pages/session/composer/WorkspacePicker.tsx",
  "src/components/scheduling/ScheduledTaskSessions.tsx",
  "src/components/session/ArtifactPreviewDrawer.tsx",
  "src/components/session/RoundArtifacts.tsx",
  "src/components/layout/session-manager/SessionActionMenu.tsx",
].map((path) => [path, readFileSync(path, "utf8")] as const);

if (!dropdownSource.includes("export type DropdownMenuEntry")) {
  throw new Error("DropdownMenu should expose a DropdownMenuEntry data type");
}

for (const prop of ["items?: DropdownMenuEntry[]", "renderItem?:"]) {
  if (!dropdownSource.includes(prop)) {
    throw new Error(`DropdownMenu should support ${prop}`);
  }
}

for (const required of [
  "DEFAULT_MAX_HEIGHT",
  "SEARCH_THRESHOLD",
  "filterDropdownEntries",
  "searchableEntries",
  "搜索",
  "maxHeight",
  "overflowY",
]) {
  if (!dropdownSource.includes(required)) {
    throw new Error(`DropdownMenu should cap tall menus and support filtering: missing ${required}`);
  }
}

if (!dropdownSource.includes("selected?: boolean")) {
  throw new Error("DropdownMenuEntry should support selected item state");
}

if (!dropdownSource.includes("children?: DropdownMenuEntry[]")) {
  throw new Error("DropdownMenuEntry item should support recursive children");
}

if (!dropdownSource.includes("tooltip?: string")) {
  throw new Error("DropdownMenuEntry item should support tooltip text");
}

if (dropdownSource.includes("description?:")) {
  throw new Error("DropdownMenuEntry and DropdownMenuItem should not expose description");
}

if (dropdownSource.includes("trailing?:")) {
  throw new Error("DropdownMenuEntry should not expose trailing ReactNode");
}

if (!dropdownSource.includes("selected") || !dropdownSource.includes('"bg-primary text-white"')) {
  throw new Error("Selected DropdownMenuItem should use primary background and white text");
}

if (!dropdownSource.includes("renderDropdownMenuEntry")) {
  throw new Error("DropdownMenu should render data entries through a shared renderer");
}

if (!dropdownSource.includes("type: \"separator\"")) {
  throw new Error("DropdownMenuEntry should support separator entries");
}

if (!dropdownSource.includes("type: \"custom\"")) {
  throw new Error("DropdownMenuEntry should support custom ReactNode entries");
}

if (!uiIndexSource.includes("DropdownMenuEntry")) {
  throw new Error("DropdownMenuEntry should be exported from the ui barrel");
}

if (!taskCardSource.includes("items={[")) {
  throw new Error("TaskCard should use data-driven DropdownMenu items");
}

if (!permissionPickerSource.includes("items={PERMISSION_MODES.map")) {
  throw new Error("PermissionPicker should use data-driven DropdownMenu items");
}

if (!dropdownSource.includes("content={entry.render")) {
  throw new Error("DropdownMenu item render should feed the item content slot");
}

if (!dropdownSource.includes("children?: ReactNode")) {
  throw new Error("DropdownMenuItem should accept custom content children");
}

if (permissionPickerSource.includes('type: "custom"')) {
  throw new Error("PermissionPicker should not render the whole item DOM");
}

if (!permissionPickerSource.includes("render: (entry)")) {
  throw new Error("PermissionPicker should render only the item content slot");
}

if (!permissionPickerSource.includes("m.detail")) {
  throw new Error("PermissionPicker should keep mode details visible in the menu");
}

for (const [path, source] of dropdownUsageSources) {
  if (!source.includes("items={")) {
    throw new Error(`${path} should use data-driven DropdownMenu items`);
  }
}

for (const [path, source] of dropdownUsageSources) {
  if (source.includes("DropdownSubMenu")) {
    throw new Error(`${path} should express submenus through DropdownMenuEntry.children`);
  }
}

for (const [path, source] of dropdownUsageSources) {
  if (source.includes("description:")) {
    throw new Error(`${path} should use tooltip or custom render instead of description`);
  }
}

const addMenuSource = readFileSync("src/pages/session/composer/AddMenu.tsx", "utf8");
if (!addMenuSource.includes("s.description") || !addMenuSource.includes("render: (entry)")) {
  throw new Error("AddMenu should render skill descriptions through custom render");
}

const sessionActionSource = readFileSync(
  "src/components/layout/session-manager/SessionActionMenu.tsx",
  "utf8",
);
if (!sessionActionSource.includes("children: [") || !sessionActionSource.includes("groups.map")) {
  throw new Error("SessionActionMenu should model group submenu through item children");
}
