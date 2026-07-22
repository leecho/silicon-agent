import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/session/composer/TeamPicker.tsx", "utf8");
const dropdownSource = readFileSync("src/components/ui/DropdownMenu.tsx", "utf8");

for (const label of ['label: "默认"', 'label: "智能体"', 'label: "团队"']) {
  if (!source.includes(label)) {
    throw new Error(`TeamPicker should expose top-level menu item ${label}`);
  }
}

if (source.includes('label: "自由模式"')) {
  throw new Error("TeamPicker should not expose the legacy free-mode wording");
}

if (!source.includes('onPick("", "")')) {
  throw new Error("TeamPicker default role item should clear the selected role");
}

if (!source.includes('"默认"')) {
  throw new Error("TeamPicker should show the default label when no team or agent is selected");
}

if (source.includes('"默认角色"') || source.includes('"选择智能体/团队"')) {
  throw new Error("TeamPicker should not keep the legacy default role or neutral picker labels");
}

if (!source.includes("children: agentItems")) {
  throw new Error("TeamPicker should render agents as a second-level menu");
}

if (!source.includes("children: teamItems")) {
  throw new Error("TeamPicker should render teams as a second-level menu");
}

if (!source.includes("width={168}")) {
  throw new Error("TeamPicker top-level menu should stay compact for short labels");
}

if (source.includes("childrenWidth: 240") || source.includes("width={256}")) {
  throw new Error("TeamPicker should not keep the oversized previous menu widths");
}

if (dropdownSource.includes("<DropdownSubMenu left={180}")) {
  throw new Error("DropdownMenu should align submenus from the parent item width, not a hard-coded 180px offset");
}

for (const legacy of ["sectionHeader", "__hdr_expert__", "专家"]) {
  if (source.includes(legacy)) {
    throw new Error(`TeamPicker should not keep legacy flat section marker ${legacy}`);
  }
}
