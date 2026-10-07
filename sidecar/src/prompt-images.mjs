import { readFile, stat } from "node:fs/promises";
import { extname, isAbsolute } from "node:path";

const IMAGE_MEDIA_TYPES = {
	".png": "image/png",
	".jpg": "image/jpeg",
	".jpeg": "image/jpeg",
	".gif": "image/gif",
	".webp": "image/webp",
};
// The Messages API rejects base64 images above 5 MB.
const MAX_IMAGE_BYTES = 5 * 1024 * 1024;
const MAX_IMAGES = 8;
const IMAGE_REFERENCE = /(?:^|\s)@(\/[^\s]+\.(?:png|jpe?g|gif|webp))(?=\s|$)/gi;

/** Absolute image paths the composer referenced as `@/path.png`, in order, once each. */
export function referencedImagePaths(prompt) {
	if (typeof prompt !== "string") return [];
	const paths = [];
	for (const match of prompt.matchAll(IMAGE_REFERENCE)) {
		const path = match[1];
		if (isAbsolute(path) && !paths.includes(path)) paths.push(path);
		if (paths.length >= MAX_IMAGES) break;
	}
	return paths;
}

/**
 * Native image blocks for the images a prompt references. The `@path` text
 * stays in the prompt so the model still knows where each file lives; the
 * block lets it see the image without spending a Read tool call. Unreadable,
 * oversized or missing files are skipped silently: the text reference remains.
 */
export async function promptImageBlocks(prompt, fs = { readFile, stat }) {
	const blocks = [];
	for (const path of referencedImagePaths(prompt)) {
		const mediaType = IMAGE_MEDIA_TYPES[extname(path).toLowerCase()];
		if (!mediaType) continue;
		try {
			const info = await fs.stat(path);
			if (!info.isFile() || info.size > MAX_IMAGE_BYTES) continue;
			const data = await fs.readFile(path);
			blocks.push({
				type: "image",
				source: { type: "base64", media_type: mediaType, data: data.toString("base64") },
			});
		} catch {
			// The textual reference is still in the prompt.
		}
	}
	return blocks;
}
