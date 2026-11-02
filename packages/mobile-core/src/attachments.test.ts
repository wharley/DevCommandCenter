import { describe, expect, it } from "vitest";
import {
	attachmentRefusal,
	base64Size,
	formatBytes,
	MAX_ATTACHMENT_BYTES,
	MAX_ATTACHMENTS,
	promptWithAttachments,
	resizedDimensions,
	splitPrompt,
} from "./attachments.ts";

describe("attachments", () => {
	it("serialize what the phone attaches so the desktop reads it back", () => {
		const paths = ["/tmp/dcc_img_1.jpg", "/tmp/dcc_file_2/notas.md"];
		const sent = promptWithAttachments("  o que tem aqui?  ", paths);
		expect(sent).toBe("o que tem aqui? @/tmp/dcc_img_1.jpg @/tmp/dcc_file_2/notas.md");
		expect(splitPrompt(sent)).toEqual({ text: "o que tem aqui?", attachments: paths });
		expect(promptWithAttachments("", paths.slice(0, 1))).toBe("@/tmp/dcc_img_1.jpg");
		expect(promptWithAttachments("só texto", [])).toBe("só texto");
	});

	it("refuse files over the limit with the sizes in Portuguese", () => {
		expect(attachmentRefusal(MAX_ATTACHMENT_BYTES, 0)).toBeNull();
		expect(attachmentRefusal(12.34 * 1024 * 1024, 0)).toBe("Este arquivo tem 12 MB; o limite é 10 MB por anexo.");
		expect(attachmentRefusal(1, MAX_ATTACHMENTS)).toBe(`Dá para anexar até ${MAX_ATTACHMENTS} arquivos por mensagem.`);
		expect(attachmentRefusal(0, 0)).toBe("Este arquivo está vazio.");
		expect(formatBytes(1.45 * 1024 * 1024)).toBe("1,5 MB");
		expect(formatBytes(300)).toBe("1 KB");
	});

	it("measure base64 and resize only photos that are too large", () => {
		expect(base64Size("aGVsbG8=")).toBe(5);
		expect(base64Size("aGVsbG8h")).toBe(6);
		expect(resizedDimensions(4032, 3024)).toEqual({ width: 2048, height: 1536 });
		expect(resizedDimensions(1170, 2532)).toEqual({ width: 946, height: 2048 });
		expect(resizedDimensions(800, 600)).toBeNull();
	});
});

describe("thread titles with attachments", async () => {
	const { threadTitle } = await import("./timeline.ts");
	it("use the written text, or the file names when there is none", () => {
		const user = (text: string) => ({ kind: "user", id: "u", text }) as never;
		expect(threadTitle([user("conserte isto @/tmp/dcc_img_1.jpg")])).toBe("conserte isto");
		expect(threadTitle([user("@/tmp/dcc_file_2/notas.md")])).toBe("notas.md");
	});
});
