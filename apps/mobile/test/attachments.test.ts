import { MAX_ATTACHMENTS, splitPrompt } from "@dcc/mobile-core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AttachmentQueue, type PendingAttachment } from "../src/attachments/queue";
import { DemoBackend } from "../src/backend/demo";
import { ThreadSync, type ThreadSnapshot } from "../src/screens/threadSync";

const JPEG = Buffer.from([0xff, 0xd8, 0xff, 0xe0, 1, 2, 3]).toString("base64");

describe("attachments", () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it("upload when picked, travel as @path and show back in the thread", async () => {
		const demo = new DemoBackend();
		let items: PendingAttachment[] = [];
		const queue = new AttachmentQueue(demo, (next) => (items = next));
		expect(queue.add({ kind: "image", name: "IMG_1.jpg", previewUri: "file:///x.jpg", base64: JPEG })).toBeNull();
		expect(items[0]?.status).toBe("uploading");
		expect(queue.uploading()).toBe(true);
		await vi.advanceTimersByTimeAsync(500);
		expect(items[0]?.status).toBe("ready");
		const path = items[0]!.path!;
		expect(path).toMatch(/dcc_img_\d+\.jpg$/);

		const prompt = queue.prompt("o que tem nesta tela?");
		expect(splitPrompt(prompt)).toEqual({ text: "o que tem nesta tela?", attachments: [path] });

		let snapshot: ThreadSnapshot | null = null;
		const sync = new ThreadSync(demo, "demo-s4", (next) => (snapshot = next));
		sync.start();
		await vi.advanceTimersByTimeAsync(1_000);
		const turn = demo.request("POST", "/api/v1/sessions/demo-s4/turns", { prompt });
		await vi.advanceTimersByTimeAsync(1_000);
		await turn;
		const sent = snapshot!.state.messages.filter((m) => m.kind === "user").at(-1);
		expect(sent && sent.kind === "user" ? splitPrompt(sent.text).attachments : []).toEqual([path]);
		const preview = demo.request<{ dataUrl: string }>(
			"GET",
			`/api/v1/mobile/sessions/demo-s4/attachment?path=${encodeURIComponent(path)}`,
		);
		await vi.advanceTimersByTimeAsync(500);
		expect((await preview).dataUrl).toBe(`data:image/jpeg;base64,${JPEG}`);
		// Another session never attached it.
		const other = demo.request("GET", `/api/v1/mobile/sessions/demo-s2/attachment?path=${encodeURIComponent(path)}`);
		const failed = expect(other).rejects.toMatchObject({ status: 404 });
		await vi.advanceTimersByTimeAsync(500);
		await failed;
		sync.stop();
		demo.dispose();
	});

	it("refuse a file over the limit before uploading, and cap the count", () => {
		const demo = new DemoBackend();
		let items: PendingAttachment[] = [];
		const queue = new AttachmentQueue(demo, (next) => (items = next));
		const big = Buffer.alloc(10 * 1024 * 1024 + 1).toString("base64");
		expect(queue.add({ kind: "file", name: "video.mov", previewUri: null, base64: big })).toBe(
			"Este arquivo tem 10 MB; o limite é 10 MB por anexo.",
		);
		expect(items).toEqual([]);
		for (let i = 0; i < MAX_ATTACHMENTS; i++) {
			expect(queue.add({ kind: "file", name: `n${i}.md`, previewUri: null, base64: "aGk=" })).toBeNull();
		}
		expect(queue.room()).toBe(0);
		expect(queue.add({ kind: "file", name: "x.md", previewUri: null, base64: "aGk=" })).toMatch(/até 6 arquivos/);
		queue.remove(items[0]!.id);
		expect(queue.room()).toBe(1);
		demo.dispose();
	});

	it("keep a failed upload out of the message", async () => {
		const failing = {
			mode: "remote",
			request: () => Promise.reject(new Error("O anexo passa de 10 MB.")),
		} as never;
		let items: PendingAttachment[] = [];
		const queue = new AttachmentQueue(failing, (next) => (items = next));
		queue.add({ kind: "file", name: "a.md", previewUri: null, base64: "aGk=" });
		await vi.advanceTimersByTimeAsync(0);
		expect(items[0]).toMatchObject({ status: "failed", error: "O anexo passa de 10 MB." });
		expect(queue.prompt("texto")).toBe("texto");
	});
});
