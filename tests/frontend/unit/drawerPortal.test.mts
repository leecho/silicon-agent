import { readFileSync } from "node:fs";

const source = readFileSync("src/components/ui/Drawer.tsx", "utf8");

for (const token of [
  'import { createPortal } from "react-dom"',
  "return createPortal(",
  "document.body",
]) {
  if (!source.includes(token)) {
    throw new Error(`Drawer should render through document.body portal to avoid transformed parent positioning: missing ${token}`);
  }
}
