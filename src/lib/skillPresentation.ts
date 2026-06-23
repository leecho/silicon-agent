import type { LucideIcon } from "lucide-react";
import {
  FileCode,
  FileCog,
  FileText,
  FileType2,
  PackagePlus,
  Presentation,
  Search,
  ShieldAlert,
  Table2,
  Wrench,
} from "lucide-react";
import type { Skill } from "../types";

export function skillIcon(skill: Skill): LucideIcon {
  const name = skill.name.toLowerCase();
  if (name.includes("find") || name.includes("search")) return Search;
  if (name.includes("docx") || name.includes("word")) return FileText;
  if (name.includes("pdf")) return FileType2;
  if (name.includes("ppt") || name.includes("presentation")) return Presentation;
  if (name.includes("xls") || name.includes("sheet") || name.includes("csv")) return Table2;
  if (name.includes("plugin")) return FileCog;
  if (name.includes("create")) return FileCode;
  if (name.includes("install") || name.includes("dependency")) return PackagePlus;
  if (name.includes("error") || name.includes("recovery")) return ShieldAlert;
  return Wrench;
}
