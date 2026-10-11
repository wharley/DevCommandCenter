/**
 * Prompt attachments, serialized the way the desktop composer does: each one
 * travels in the prompt as `@/absolute/path` on the Mac
 * (`apps/desktop/src/lib/prompt-attachments.ts`), and providers with native
 * image input attach the images it references. The phone uploads the file
 * first (`POST /api/v1/mobile/attachments`) and gets that path back.
 * `attachments.test.ts` checks the parsing against the desktop's own code.
 */

export type PromptPart = { kind: "text"; value: string } | { kind: "attachment"; path: string };

/** Same limit as the desktop route (`MAX_PHONE_ATTACHMENT_BYTES`). */
export const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024;
export const MAX_ATTACHMENTS = 6;
/** Longest edge of a photo after the phone resizes it. */
export const IMAGE_MAX_EDGE = 2048;
export const IMAGE_JPEG_QUALITY = 0.8;

/** Splits a sent prompt into text and attachments (copy of the desktop's `promptParts`). */
export function promptParts(content: string): PromptPart[] {
	const parts: PromptPart[] = [];
	// Paths can contain spaces, so consume through the filename extension instead
	// of stopping at the first whitespace before the rest of the prompt.
	const attachmentPattern = /@((?:\/|[A-Za-z]:[\\/]|\\\\)[^\r\n<>]*?\.[a-z\d]{1,12})(?=$|[\s),.;!?])/gi;
	let cursor = 0;
	for (const match of content.matchAll(attachmentPattern)) {
		const path = match[1] ?? "";
		const start = match.index ?? 0;
		if (start > cursor) parts.push({ kind: "text", value: content.slice(cursor, start) });
		parts.push({ kind: "attachment", path });
		cursor = start + 1 + path.length;
	}
	if (cursor < content.length) parts.push({ kind: "text", value: content.slice(cursor) });
	return parts.length ? parts : [{ kind: "text", value: content }];
}

/** A sent prompt as the bubble shows it: the text, and the attachments above it. */
export function splitPrompt(content: string): { text: string; attachments: string[] } {
	const parts = promptParts(content);
	return {
		text: parts
			.map((part) => (part.kind === "text" ? part.value : ""))
			.join("")
			.trim(),
		attachments: parts.flatMap((part) => (part.kind === "attachment" ? [part.path] : [])),
	};
}

/**
 * What the phone sends: the typed text, then each attachment as `@path`
 * separated by a space — what the desktop composer produces when files are
 * attached after the text.
 */
export function promptWithAttachments(text: string, paths: readonly string[]): string {
	const body = text.trim();
	const refs = paths.map((path) => `@${path}`).join(" ");
	if (!refs) return body;
	return body ? `${body} ${refs}` : refs;
}

const IMAGE_FILE_RE = /\.(?:png|jpe?g|gif|webp|svg|bmp|ico)$/i;

/** The desktop's `isImageFilePath`: a path (with a separator) to an image file. */
export function isImagePath(path: string): boolean {
	const trimmed = path.trim();
	if (trimmed.length < 5 || !IMAGE_FILE_RE.test(trimmed)) return false;
	return trimmed.includes("/") || trimmed.includes("\\");
}

export function pathBasename(path: string): string {
	const normalized = path.replace(/\\/g, "/");
	const index = normalized.lastIndexOf("/");
	return index >= 0 ? normalized.slice(index + 1) : normalized;
}

/** "1,4 MB", "820 KB": sizes as the person reads them. */
export function formatBytes(bytes: number): string {
	if (bytes >= 1024 * 1024) {
		const mb = bytes / (1024 * 1024);
		return `${mb.toLocaleString("pt-BR", { maximumFractionDigits: mb < 10 ? 1 : 0 })} MB`;
	}
	return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

/** Why a file cannot be attached, or `null` when it can. */
export function attachmentRefusal(size: number, alreadyAttached: number): string | null {
	if (alreadyAttached >= MAX_ATTACHMENTS) {
		return `Dá para anexar até ${MAX_ATTACHMENTS} arquivos por mensagem.`;
	}
	if (size > MAX_ATTACHMENT_BYTES) {
		return `Este arquivo tem ${formatBytes(size)}; o limite é ${formatBytes(MAX_ATTACHMENT_BYTES)} por anexo.`;
	}
	if (size <= 0) return "Este arquivo está vazio.";
	return null;
}

/** Decoded size of a base64 string, without decoding it. */
export function base64Size(base64: string): number {
	const length = base64.replace(/\s/g, "").length;
	const padding = base64.endsWith("==") ? 2 : base64.endsWith("=") ? 1 : 0;
	return Math.max(0, Math.floor((length * 3) / 4) - padding);
}

/**
 * The size a photo is resized to before upload: its longest edge capped at
 * `maxEdge`, proportions kept; `null` when it already fits.
 */
export function resizedDimensions(
	width: number,
	height: number,
	maxEdge = IMAGE_MAX_EDGE,
): { width: number; height: number } | null {
	const edge = Math.max(width, height);
	if (!(edge > maxEdge)) return null;
	const scale = maxEdge / edge;
	return { width: Math.round(width * scale), height: Math.round(height * scale) };
}
