import type { SkillFile } from "../../types";

/** 文件列表排序：SKILL.md 始终置顶；目录在前，同类按 relPath 字典序。 */
export function sortSkillFiles(files: SkillFile[]): SkillFile[] {
  return [...files].sort((a, b) => {
    if (a.relPath === "SKILL.md") return -1;
    if (b.relPath === "SKILL.md") return 1;
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return a.relPath.localeCompare(b.relPath);
  });
}
