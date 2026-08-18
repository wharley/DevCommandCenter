export type WorkspaceFileReference = {
	path: string;
	line: number | null;
	column: number | null;
};

function decodeLinkTarget(value: string) {
	try {
		return decodeURIComponent(value);
	} catch {
		return value;
	}
}

function normalizeSlashes(value: string) {
	return value.replace(/\\/g, "/");
}

function stripPosition(value: string) {
	const fragmentMatch = /^(.*)#L(\d+)(?:C(\d+))?$/i.exec(value);
	if (fragmentMatch) {
		return {
			path: fragmentMatch[1],
			line: Number(fragmentMatch[2]),
			column: fragmentMatch[3] ? Number(fragmentMatch[3]) : null,
		};
	}

	const positionMatch = /^(.*):(\d+):(\d+)$/.exec(value);
	if (positionMatch) {
		return {
			path: positionMatch[1],
			line: Number(positionMatch[2]),
			column: Number(positionMatch[3]),
		};
	}

	const suffixMatch = /^(.*):(\d+)$/.exec(value);
	if (suffixMatch) {
		return {
			path: suffixMatch[1],
			line: Number(suffixMatch[2]),
			column: null,
		};
	}

	return { path: value, line: null, column: null };
}

function isAbsolutePath(value: string) {
	return value.startsWith("/") || /^[a-zA-Z]:\//.test(value);
}

function pathsEqualOrNested(path: string, root: string, caseInsensitive: boolean) {
	const candidate = caseInsensitive ? path.toLowerCase() : path;
	const boundary = caseInsensitive ? root.toLowerCase() : root;
	return candidate === boundary || candidate.startsWith(`${boundary}/`);
}

export function parseWorkspaceFileReference(
	href: string | undefined,
	workspaceRoot: string | null | undefined,
): WorkspaceFileReference | null {
	const rootValue = workspaceRoot?.trim();
	const hrefValue = href?.trim();
	if (!rootValue || !hrefValue || hrefValue.startsWith("#")) {
		return null;
	}

	if (
		/^[a-z][a-z0-9+.-]*:/i.test(hrefValue) &&
		!/^file:/i.test(hrefValue) &&
		!/^[a-zA-Z]:[\\/]/.test(hrefValue)
	) {
		return null;
	}

	const withoutScheme = hrefValue.startsWith("file://")
		? hrefValue.slice("file://".length)
		: hrefValue;
	const decoded = normalizeSlashes(decodeLinkTarget(withoutScheme));
	const positioned = stripPosition(decoded);
	let candidate = positioned.path.trim();
	if (!candidate) {
		return null;
	}

	const root = normalizeSlashes(rootValue).replace(/\/+$/, "");
	const windowsRoot = /^[a-zA-Z]:\//.test(root);
	if (windowsRoot && /^\/[a-zA-Z]:\//.test(candidate)) {
		candidate = candidate.slice(1);
	}

	let relativePath: string;
	if (isAbsolutePath(candidate)) {
		if (!pathsEqualOrNested(candidate, root, windowsRoot)) {
			return null;
		}
		relativePath = candidate.slice(root.length).replace(/^\/+/, "");
	} else {
		relativePath = candidate.replace(/^\.\//, "");
	}

	const segments = relativePath.split("/");
	if (
		!relativePath ||
		segments.some((segment) => !segment || segment === "." || segment === "..")
	) {
		return null;
	}

	return {
		path: relativePath,
		line: positioned.line,
		column: positioned.column,
	};
}
