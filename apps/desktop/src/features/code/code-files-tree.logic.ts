export type CodeTreeNode = {
	name: string;
	path: string;
	children: Map<string, CodeTreeNode>;
	file?: string;
};

export function buildCodeTree(paths: readonly string[]): CodeTreeNode {
	const root: CodeTreeNode = { name: "", path: "", children: new Map() };
	for (const filePath of paths) {
		const parts = filePath.split("/").filter(Boolean);
		if (parts.length === 0) continue;
		let current = root;
		for (let index = 0; index < parts.length - 1; index += 1) {
			const part = parts[index]!;
			let next = current.children.get(part);
			if (!next) {
				next = {
					name: part,
					path: parts.slice(0, index + 1).join("/"),
					children: new Map(),
				};
				current.children.set(part, next);
			}
			current = next;
		}
		const fileName = parts[parts.length - 1]!;
		current.children.set(fileName, {
			name: fileName,
			path: filePath,
			children: new Map(),
			file: filePath,
		});
	}
	return root;
}

export type CodeTreeRow =
	| {
			kind: "folder";
			/** Deepest folder of a compacted chain; the key for expansion. */
			path: string;
			/** `src/features` when a folder only holds one folder. */
			label: string;
			depth: number;
			expanded: boolean;
			parentPath: string | null;
	  }
	| {
			kind: "file";
			path: string;
			label: string;
			depth: number;
			parentPath: string | null;
	  };

function isFolder(node: CodeTreeNode): boolean {
	return !node.file;
}

function sortedChildren(node: CodeTreeNode): CodeTreeNode[] {
	return [...node.children.values()].sort((left, right) => {
		if (isFolder(left) !== isFolder(right)) return isFolder(left) ? -1 : 1;
		return left.name.localeCompare(right.name);
	});
}

/** The rows on screen, top to bottom, for the folders that are open. */
export function visibleCodeTreeRows(
	root: CodeTreeNode,
	expanded: ReadonlySet<string>,
): CodeTreeRow[] {
	const rows: CodeTreeRow[] = [];
	const walk = (node: CodeTreeNode, depth: number, parentPath: string | null) => {
		for (const child of sortedChildren(node)) {
			if (!isFolder(child)) {
				rows.push({
					kind: "file",
					path: child.path,
					label: child.name,
					depth,
					parentPath,
				});
				continue;
			}
			// A folder whose only entry is another folder reads as one row.
			let folder = child;
			let label = child.name;
			while (folder.children.size === 1) {
				const only = folder.children.values().next().value as CodeTreeNode;
				if (!isFolder(only)) break;
				folder = only;
				label = `${label}/${only.name}`;
			}
			const open = expanded.has(folder.path);
			rows.push({
				kind: "folder",
				path: folder.path,
				label,
				depth,
				expanded: open,
				parentPath,
			});
			if (open) walk(folder, depth + 1, folder.path);
		}
	};
	walk(root, 0, null);
	return rows;
}

/** Every folder above a file, so revealing it opens the way down. */
export function ancestorFolderPaths(filePath: string): string[] {
	const parts = filePath.split("/").filter(Boolean);
	const folders: string[] = [];
	for (let index = 1; index < parts.length; index += 1) {
		folders.push(parts.slice(0, index).join("/"));
	}
	return folders;
}

export type CodeTreeGitStatus = {
	/** Display letter: M, A, D, R, U (new, untracked) or ! (conflict). */
	letter: string;
	tone: "modified" | "added" | "deleted" | "renamed" | "conflict";
};

type GitChange = { path: string; status: string };

function toTreeStatus(status: string): CodeTreeGitStatus | null {
	switch (status) {
		case "M":
			return { letter: "M", tone: "modified" };
		case "A":
			return { letter: "A", tone: "added" };
		case "?":
			return { letter: "U", tone: "added" };
		case "D":
			return { letter: "D", tone: "deleted" };
		case "R":
		case "C":
			return { letter: status, tone: "renamed" };
		case "U":
			return { letter: "!", tone: "conflict" };
		default:
			return null;
	}
}

/**
 * Per-file status for the tree (the worktree side wins over the index) and
 * the folders that hold a change.
 */
export function codeTreeGitStatus(
	staged: readonly GitChange[],
	unstaged: readonly GitChange[],
): { byPath: Map<string, CodeTreeGitStatus>; changedFolders: Set<string> } {
	const byPath = new Map<string, CodeTreeGitStatus>();
	for (const entry of [...staged, ...unstaged]) {
		const status = toTreeStatus(entry.status);
		if (status) byPath.set(entry.path, status);
	}
	const changedFolders = new Set<string>();
	for (const path of byPath.keys()) {
		for (const folder of ancestorFolderPaths(path)) changedFolders.add(folder);
	}
	return { byPath, changedFolders };
}

/**
 * Changes that add or remove files. When this moves, the file list is stale
 * (an agent created or deleted something).
 */
export function fileSetSignature(
	staged: readonly GitChange[],
	unstaged: readonly GitChange[],
): string {
	return [...staged, ...unstaged]
		.filter((entry) => entry.status !== "M")
		.map((entry) => `${entry.status}:${entry.path}`)
		.sort()
		.join("\n");
}
