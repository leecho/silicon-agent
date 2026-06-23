import { readFileSync } from "node:fs";

const dropdownSource = readFileSync("src/components/ui/DropdownMenu.tsx", "utf8");
const modalSource = readFileSync("src/components/ui/Modal.tsx", "utf8");

function parseZIndex(classNames: string): number {
  const arbitrary = classNames.match(/\bz-\[(\d+)\]/);
  if (arbitrary) return Number(arbitrary[1]);

  const scale = classNames.match(/\bz-(\d+)\b/);
  if (scale) return Number(scale[1]);

  throw new Error(`Could not find z-index class in: ${classNames}`);
}

const dropdownRootClass = dropdownSource.match(/ref=\{menuRef\}\s+className="([^"]+)"/)?.[1];
if (!dropdownRootClass) {
  throw new Error("Could not locate DropdownMenu root className");
}

const modalOverlayClass = modalSource.match(/className="([^"]*fixed inset-0[^"]*)"/)?.[1];
if (!modalOverlayClass) {
  throw new Error("Could not locate Modal overlay className");
}

const dropdownZIndex = parseZIndex(dropdownRootClass);
const modalZIndex = parseZIndex(modalOverlayClass);

if (dropdownZIndex <= modalZIndex) {
  throw new Error(
    `DropdownMenu z-index (${dropdownZIndex}) must be greater than Modal z-index (${modalZIndex})`,
  );
}
