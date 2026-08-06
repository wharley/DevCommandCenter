export type FileContentCacheStats = {
	entries: number;
	bytes: number;
};

export class FileContentLru {
	private readonly entries = new Map<string, string>();
	private bytes = 0;

	constructor(
		private readonly maxEntries = 32,
		private readonly maxBytes = 8 * 1024 * 1024,
	) {}

	get(path: string) {
		const value = this.entries.get(path);
		if (value === undefined) return undefined;
		this.entries.delete(path);
		this.entries.set(path, value);
		return value;
	}

	set(path: string, content: string) {
		const previous = this.entries.get(path);
		if (previous !== undefined) {
			this.bytes -= FileContentLru.sizeOf(previous);
			this.entries.delete(path);
		}
		this.entries.set(path, content);
		this.bytes += FileContentLru.sizeOf(content);
		this.evict();
	}

	purgeRoot(root: string) {
		const normalized = FileContentLru.normalizePath(root);
		for (const [path, content] of this.entries) {
			const normalizedPath = FileContentLru.normalizePath(path);
			if (
				normalizedPath === normalized ||
				normalizedPath.startsWith(`${normalized}/`)
			) {
				this.entries.delete(path);
				this.bytes -= FileContentLru.sizeOf(content);
			}
		}
	}

	stats(): FileContentCacheStats {
		return { entries: this.entries.size, bytes: this.bytes };
	}

	private evict() {
		while (this.entries.size > this.maxEntries || this.bytes > this.maxBytes) {
			const oldest = this.entries.entries().next().value as
				| [string, string]
				| undefined;
			if (!oldest) break;
			this.entries.delete(oldest[0]);
			this.bytes -= FileContentLru.sizeOf(oldest[1]);
		}
	}

	private static sizeOf(value: string) {
		return value.length * 2;
	}

	private static normalizePath(value: string) {
		return value.replace(/\\/g, "/").replace(/\/+$/, "");
	}
}

export const monacoFileContentCache = new FileContentLru();

export function purgeMonacoFileContentsForRoot(root: string) {
	monacoFileContentCache.purgeRoot(root);
}
