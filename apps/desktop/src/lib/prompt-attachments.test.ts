import { describe, expect, it } from "vitest";
import { promptParts } from "./prompt-attachments";

describe("promptParts", () => {
	it("splits a workspace mention with route-group and dynamic segments", () => {
		expect(
			promptParts("ajusta @src/app/(private)/admin/[storeId]/collections/page.tsx para paginar"),
		).toEqual([
			{ kind: "text", value: "ajusta " },
			{ kind: "mention", path: "src/app/(private)/admin/[storeId]/collections/page.tsx" },
			{ kind: "text", value: " para paginar" },
		]);
	});

	it("keeps the whole multi-dot filename and leaves sentence punctuation as text", () => {
		expect(promptParts("veja @src/a.test.ts.")).toEqual([
			{ kind: "text", value: "veja " },
			{ kind: "mention", path: "src/a.test.ts" },
			{ kind: "text", value: "." },
		]);
		expect(promptParts("(@README.md), ok")).toEqual([
			{ kind: "text", value: "(" },
			{ kind: "mention", path: "README.md" },
			{ kind: "text", value: "), ok" },
		]);
	});

	it("does not take handles, scoped packages or e-mails for files", () => {
		for (const text of ["fala @coderabbitai", "instala @types/node", "manda pra eu@site.com"]) {
			expect(promptParts(text)).toEqual([{ kind: "text", value: text }]);
		}
	});

	it("leaves mentions inside code literal", () => {
		const text = "rode `cat @src/a.ts` e\n```sh\nls @src/b.ts\n```\nmas olhe @src/c.ts";
		const mentions = promptParts(text).filter((part) => part.kind === "mention");
		expect(mentions).toEqual([{ kind: "mention", path: "src/c.ts" }]);
	});

	it("still reads absolute attachments, spaces included", () => {
		expect(promptParts("@/tmp/My Shots/dcc_img_1.png olha @src/x.ts")).toEqual([
			{ kind: "attachment", path: "/tmp/My Shots/dcc_img_1.png" },
			{ kind: "text", value: " olha " },
			{ kind: "mention", path: "src/x.ts" },
		]);
	});
});
