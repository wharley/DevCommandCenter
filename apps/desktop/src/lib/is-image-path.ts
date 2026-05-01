const IMAGE_FILE_RE = /\.(?:png|jpe?g|gif|webp|svg|bmp|ico)$/i;

/** True for lines that look like a filesystem path to a raster/vector image. */
export function isImageFilePath(line: string): boolean {
	const t = line.trim();
	if (t.length < 5) {
		return false;
	}
	if (!IMAGE_FILE_RE.test(t)) {
		return false;
	}
	return t.includes("/") || t.includes("\\");
}
