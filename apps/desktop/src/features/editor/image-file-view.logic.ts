/** Formats the app can read back as an image (same list as `read_image_preview`). */
const VIEWABLE_IMAGE_RE = /\.(?:png|jpe?g|gif|webp)$/i;

/** True when a file tab should show the picture instead of the text editor. */
export function isViewableImagePath(path: string): boolean {
	return VIEWABLE_IMAGE_RE.test(path.trim());
}

/** Absolute path of a workspace file; tab paths are relative to the root. */
export function workspaceAbsolutePath(root: string, path: string): string {
	if (path.startsWith("/") || /^[A-Za-z]:[\\/]/.test(path)) return path;
	const base = root.replace(/[\\/]+$/, "");
	const rel = path.replace(/^\.?[\\/]+/, "");
	return `${base}/${rel}`;
}

export const MIN_IMAGE_ZOOM = 0.05;
export const MAX_IMAGE_ZOOM = 16;

const ZOOM_STEPS = [
	0.05, 0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 1, 1.25, 1.5, 2, 3, 4, 6, 8, 12, 16,
];

export function clampImageZoom(zoom: number): number {
	return Math.min(MAX_IMAGE_ZOOM, Math.max(MIN_IMAGE_ZOOM, zoom));
}

/** Scale that fits the whole image in the box, never enlarging small images. */
export function fitImageZoom(
	image: { width: number; height: number },
	box: { width: number; height: number },
): number {
	if (
		image.width <= 0 ||
		image.height <= 0 ||
		box.width <= 0 ||
		box.height <= 0
	) {
		return 1;
	}
	return clampImageZoom(
		Math.min(1, box.width / image.width, box.height / image.height),
	);
}

/** Next preset step in or out from the current zoom (buttons and ⌘+/⌘−). */
export function stepImageZoom(zoom: number, direction: 1 | -1): number {
	if (direction > 0) {
		return ZOOM_STEPS.find((step) => step > zoom + 1e-6) ?? MAX_IMAGE_ZOOM;
	}
	return (
		[...ZOOM_STEPS].reverse().find((step) => step < zoom - 1e-6) ??
		MIN_IMAGE_ZOOM
	);
}

export function formatImageBytes(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
	return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Decoded size of a base64 data URL, without decoding it. */
export function dataUrlByteLength(dataUrl: string): number {
	const comma = dataUrl.indexOf(",");
	if (comma < 0) return 0;
	const body = dataUrl.slice(comma + 1);
	const padding = body.endsWith("==") ? 2 : body.endsWith("=") ? 1 : 0;
	return Math.max(0, Math.floor((body.length * 3) / 4) - padding);
}
