/**
 * Files attached to a message before it is sent. Each one uploads as soon as
 * it is picked (`POST /api/v1/mobile/attachments`, through the encrypted
 * channel like any request) and comes back as a path on the computer; the
 * message then carries `@path`, as the desktop composer writes it. Removing a
 * thumbnail just leaves it out of the message.
 */
import { attachmentRefusal, base64Size, MAX_ATTACHMENTS, promptWithAttachments } from "@dcc/mobile-core";
import type { Backend } from "../backend/types";
import { errorMessage } from "../backend/types";

/** A file read on the phone, ready to upload. */
export type PreparedAttachment = {
	kind: "image" | "file";
	name: string;
	/** Local URI for the thumbnail (images). */
	previewUri: string | null;
	base64: string;
};

export type PendingAttachment = {
	id: string;
	kind: "image" | "file";
	name: string;
	previewUri: string | null;
	size: number;
	status: "uploading" | "ready" | "failed";
	/** Where the computer saved it (`@path` in the prompt). */
	path: string | null;
	error: string | null;
};

type Uploaded = { kind: "image" | "file"; path: string; name: string | null; size: number };

export class AttachmentQueue {
	private items: PendingAttachment[] = [];
	private next = 0;

	constructor(
		private readonly backend: Backend,
		private readonly onChange: (items: PendingAttachment[]) => void,
	) {}

	list(): PendingAttachment[] {
		return this.items;
	}

	room(): number {
		return Math.max(0, MAX_ATTACHMENTS - this.items.length);
	}

	/** Refusal message (size, count) or `null` once the upload started. */
	add(file: PreparedAttachment): string | null {
		const size = base64Size(file.base64);
		const refusal = attachmentRefusal(size, this.items.length);
		if (refusal) return refusal;
		const id = `att-${++this.next}`;
		this.items = [
			...this.items,
			{ id, kind: file.kind, name: file.name, previewUri: file.previewUri, size, status: "uploading", path: null, error: null },
		];
		this.emit();
		void this.upload(id, file);
		return null;
	}

	remove(id: string): void {
		this.items = this.items.filter((item) => item.id !== id);
		this.emit();
	}

	clear(): void {
		this.items = [];
		this.emit();
	}

	uploading(): boolean {
		return this.items.some((item) => item.status === "uploading");
	}

	/** The message to send: text plus every uploaded attachment. */
	prompt(text: string): string {
		return promptWithAttachments(
			text,
			this.items.flatMap((item) => (item.status === "ready" && item.path ? [item.path] : [])),
		);
	}

	hasFailed(): boolean {
		return this.items.some((item) => item.status === "failed");
	}

	private async upload(id: string, file: PreparedAttachment) {
		try {
			const saved = await this.backend.request<Uploaded>("POST", "/api/v1/mobile/attachments", {
				name: file.name,
				data: file.base64,
			});
			this.update(id, { status: "ready", path: saved.path, kind: saved.kind });
		} catch (error) {
			this.update(id, { status: "failed", error: errorMessage(error, "Falha ao enviar o anexo.") });
		}
	}

	private update(id: string, patch: Partial<PendingAttachment>) {
		if (!this.items.some((item) => item.id === id)) return;
		this.items = this.items.map((item) => (item.id === id ? { ...item, ...patch } : item));
		this.emit();
	}

	private emit() {
		this.onChange(this.items);
	}
}
