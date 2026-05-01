import { invoke } from "@tauri-apps/api/core";

export function mimeTypeToImageExtension(mime: string): string {
	const m = mime.toLowerCase();
	if (m === "image/jpeg" || m === "image/jpg") {
		return "jpg";
	}
	if (m === "image/png") {
		return "png";
	}
	if (m === "image/gif") {
		return "gif";
	}
	if (m === "image/webp") {
		return "webp";
	}
	if (m === "image/svg+xml") {
		return "svg";
	}
	if (m === "image/bmp") {
		return "bmp";
	}
	return "png";
}

/** Persists clipboard image bytes to the OS temp dir (same as `terminal_save_temp_image`). */
export async function saveClipboardImageToTempFile(file: File): Promise<string> {
	const buffer = await file.arrayBuffer();
	const imageData = Array.from(new Uint8Array(buffer));
	const extension = mimeTypeToImageExtension(file.type);
	const result = await invoke<{ path: string; filename: string }>(
		"terminal_save_temp_image",
		{ imageData, extension },
	);
	return result.path;
}
