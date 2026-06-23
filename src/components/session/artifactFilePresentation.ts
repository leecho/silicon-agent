import type { LucideIcon } from "lucide-react";
import {
  File,
  FileArchive,
  FileCode,
  FileImage,
  FileText,
  Table,
} from "lucide-react";

export function artifactFileName(path: string): string {
  const normalized = path.replace(/[/\\]+$/, "");
  const idx = Math.max(normalized.lastIndexOf("/"), normalized.lastIndexOf("\\"));
  return idx >= 0 ? normalized.slice(idx + 1) : normalized;
}

export function artifactFullPath(
  resolvedWorkingDir: string | undefined,
  artifactPath: string | undefined,
): string | undefined {
  if (!resolvedWorkingDir || !artifactPath) return undefined;
  const root = resolvedWorkingDir.replace(/[/\\]+$/, "");
  const rel = artifactPath.replace(/^[/\\]+/, "");
  return `${root}/${rel}`;
}

function artifactExtension(path: string): string {
  const fileName = artifactFileName(path);
  const idx = fileName.lastIndexOf(".");
  return idx > 0 ? fileName.slice(idx + 1).toLowerCase() : "";
}

export function artifactIcon(path: string): LucideIcon {
  const ext = artifactExtension(path);
  if (["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"].includes(ext)) {
    return FileImage;
  }
  if (["zip", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar"].includes(ext)) {
    return FileArchive;
  }
  if (["csv", "tsv", "xls", "xlsx"].includes(ext)) {
    return Table;
  }
  if (
    [
      "c",
      "cc",
      "cpp",
      "css",
      "go",
      "html",
      "java",
      "js",
      "jsx",
      "json",
      "kt",
      "mdx",
      "rs",
      "sh",
      "sql",
      "swift",
      "toml",
      "ts",
      "tsx",
      "xml",
      "yaml",
      "yml",
    ].includes(ext)
  ) {
    return FileCode;
  }
  if (["doc", "docx", "log", "md", "pdf", "rtf", "txt"].includes(ext)) {
    return FileText;
  }
  return File;
}
