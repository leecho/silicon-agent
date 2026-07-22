// 工作空间文件树节点。dir 有 children；file 为叶子。path 为工作空间相对路径（正斜杠）。
export interface WorkspaceTreeNode {
  name: string;
  path: string;
  type: "dir" | "file";
  children?: WorkspaceTreeNode[];
}

// 把扁平相对路径数组（如 ["a/b.txt", "a/c.txt", "d.txt"]）构建成目录树。
// 规则：目录在前、文件在后；同类按 name 本地化排序（稳定、与后端返回顺序无关）。
export function buildWorkspaceTree(paths: string[]): WorkspaceTreeNode[] {
  const root: WorkspaceTreeNode = { name: "", path: "", type: "dir", children: [] };

  for (const raw of paths) {
    const rel = raw.replace(/^\/+/, "").replace(/\/+$/, "");
    if (!rel) continue;
    const segments = rel.split("/");
    let cursor = root;
    segments.forEach((seg, i) => {
      const isLeaf = i === segments.length - 1;
      const nodePath = segments.slice(0, i + 1).join("/");
      const children = cursor.children!;
      let next = children.find((c) => c.name === seg);
      if (!next) {
        next = {
          name: seg,
          path: nodePath,
          type: isLeaf ? "file" : "dir",
          ...(isLeaf ? {} : { children: [] }),
        };
        children.push(next);
      } else if (!isLeaf && next.type === "file") {
        // 极少数冲突（同名既作文件又作目录前缀）：以目录为准。
        next.type = "dir";
        next.children = next.children ?? [];
      }
      cursor = next;
    });
  }

  sortTree(root.children!);
  return root.children!;
}

function sortTree(nodes: WorkspaceTreeNode[]): void {
  nodes.sort((a, b) => {
    if (a.type !== b.type) return a.type === "dir" ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
  for (const n of nodes) if (n.children) sortTree(n.children);
}
