import { readFileSync } from "node:fs";

const source = readFileSync("src/components/session/RoundArtifacts.tsx", "utf8");

const splitButtonStart = source.indexOf('className="flex h-8 shrink-0 overflow-hidden');
const menuStart = source.indexOf("{menuArtifact && artifactPath(menuArtifact)", splitButtonStart);

if (splitButtonStart === -1 || menuStart === -1) {
  throw new Error("Could not locate the artifact action split button");
}

const splitButtonSource = source.slice(splitButtonStart, menuStart);

if (!splitButtonSource.includes('content="预览"')) {
  throw new Error("Artifact right-side primary action should be Preview");
}

if (!splitButtonSource.includes("onClick={() => onOpen?.(a)}")) {
  throw new Error("Artifact right-side primary action should open the preview drawer");
}

if (splitButtonSource.includes("onClick={() => void handleOpenFile(a)}")) {
  throw new Error("Artifact Open action should live in the dropdown menu, not the primary button");
}

const menuSource = source.slice(menuStart);

if (!menuSource.includes('label="打开"')) {
  throw new Error("Artifact dropdown menu should include the Open action");
}

if (!menuSource.includes("onClick={() => void handleOpenFile(menuArtifact)}")) {
  throw new Error("Artifact dropdown Open action should call handleOpenFile");
}
