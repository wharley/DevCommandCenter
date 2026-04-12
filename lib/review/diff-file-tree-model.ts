/**
 * Builds a directory tree from flat changed-file paths for diff navigation.
 */

export type DiffFileTreeItem = {
  path: string;
  status: string;
  insertions?: number;
  deletions?: number;
};

export type DiffTreeDir = {
  kind: "dir";
  /** Last segment only (folder name). */
  segment: string;
  /** Full path prefix including this folder (no trailing slash). */
  fullPath: string;
  children: DiffTreeNode[];
};

export type DiffTreeFile = {
  kind: "file";
  segment: string;
  fullPath: string;
  meta: DiffFileTreeItem;
};

export type DiffTreeNode = DiffTreeDir | DiffTreeFile;

function compareSegment(a: DiffTreeNode, b: DiffTreeNode): number {
  return a.segment.localeCompare(b.segment, undefined, {
    sensitivity: "base",
  });
}

function ensureDir(
  root: DiffTreeDir,
  segments: string[],
  depth: number,
): DiffTreeDir {
  if (depth >= segments.length - 1) {
    return root;
  }
  const seg = segments[depth]!;
  const prefix = segments.slice(0, depth + 1).join("/");
  let next = root.children.find(
    (c): c is DiffTreeDir => c.kind === "dir" && c.segment === seg,
  );
  if (!next) {
    next = {
      kind: "dir",
      segment: seg,
      fullPath: prefix,
      children: [],
    };
    root.children.push(next);
  }
  return ensureDir(next, segments, depth + 1);
}

/**
 * Groups flat file paths into a nested tree (directories are expanded by the UI).
 */
export function buildDiffFileTree(files: DiffFileTreeItem[]): DiffTreeNode[] {
  const virtualRoot: DiffTreeDir = {
    kind: "dir",
    segment: "",
    fullPath: "",
    children: [],
  };

  const sorted = [...files].sort((a, b) =>
    a.path.localeCompare(b.path, undefined, { sensitivity: "base" }),
  );

  for (const meta of sorted) {
    const parts = meta.path.replace(/\\/g, "/").split("/").filter(Boolean);
    if (parts.length === 0) continue;

    if (parts.length === 1) {
      virtualRoot.children.push({
        kind: "file",
        segment: parts[0]!,
        fullPath: meta.path,
        meta,
      });
      continue;
    }

    const dir = ensureDir(virtualRoot, parts, 0);
    const fileName = parts[parts.length - 1]!;
    dir.children.push({
      kind: "file",
      segment: fileName,
      fullPath: meta.path,
      meta,
    });
  }

  function sortRecursive(node: DiffTreeDir): void {
    node.children.sort(compareSegment);
    for (const c of node.children) {
      if (c.kind === "dir") sortRecursive(c);
    }
  }
  sortRecursive(virtualRoot);

  return virtualRoot.children;
}
