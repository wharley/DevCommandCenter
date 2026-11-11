import { describe, expect, it } from "vitest";
import { inlinePieces, plainMarkdown, safeUrl, shortUrl } from "./markdownInline.ts";

describe("inline markdown", () => {
	it("reads a link by its label and keeps code and bold", () => {
		expect(inlinePieces("Veja a [fatura 181637](https://app.zedy.com.br/admin/27885/invoice/181637) em `prod` **agora**.")).toEqual([
			{ kind: "text", text: "Veja a " },
			{ kind: "link", text: "fatura 181637", url: "https://app.zedy.com.br/admin/27885/invoice/181637" },
			{ kind: "text", text: " em " },
			{ kind: "code", text: "prod" },
			{ kind: "text", text: " " },
			{ kind: "bold", text: "agora" },
			{ kind: "text", text: "." },
		]);
	});

	it("shortens a bare address and leaves the sentence's period out", () => {
		const pieces = inlinePieces("Thread: https://zedy-group.slack.com/archives/C0ALF3UJTD4/p1794432873322989.");
		expect(pieces[1]).toEqual({ kind: "link", text: "zedy-group.slack.com/archives/C0ALF…", url: "https://zedy-group.slack.com/archives/C0ALF3UJTD4/p1794432873322989" });
		expect(pieces[2]).toEqual({ kind: "text", text: "." });
	});

	it("never opens what is not a web or mail address", () => {
		// An unsafe link shows its label only, as text: nothing to tap.
		expect(inlinePieces("[x](javascript:void)")).toEqual([{ kind: "text", text: "x" }]);
		expect(safeUrl("file:///etc/passwd")).toBeNull();
		expect(safeUrl("mailto:a@b.com")).toBe("mailto:a@b.com");
		expect(shortUrl("https://a.com/")).toBe("a.com");
	});

	it("turns a message into one plain line for previews", () => {
		expect(plainMarkdown("↪ Suporte → Dev: @Dev Audite a **cobrança** da [fatura 181637](https://app.zedy.com.br/x)\n\nem `prod`")).toBe(
			"↪ Suporte → Dev: @Dev Audite a cobrança da fatura 181637 em prod",
		);
	});
});
