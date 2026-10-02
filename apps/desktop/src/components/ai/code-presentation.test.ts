import { describe, expect, it } from "vitest";
import {
	fenceBareJson,
	fenceLanguageForPath,
	fenceStartLine,
	formatJsonForDisplay,
	resolveCodeLanguage,
} from "./code-presentation";

describe("resolveCodeLanguage", () => {
	it("keeps a named language and treats text labels as plain", () => {
		expect(resolveCodeLanguage("TS", "const a = 1;")).toBe("ts");
		expect(resolveCodeLanguage("text", "const a = 1;")).toBeNull();
		expect(resolveCodeLanguage(undefined, "const a = 1;")).toBeNull();
	});

	it("reads an unlabelled fence holding valid JSON as json", () => {
		expect(resolveCodeLanguage(undefined, '{"a": [1, 2]}')).toBe("json");
		expect(resolveCodeLanguage("text", "{ not json }")).toBeNull();
		// Detection is off while the fence is still streaming.
		expect(resolveCodeLanguage(undefined, '{"a": 1}', false)).toBeNull();
	});
});

describe("formatJsonForDisplay", () => {
	it("re-indents single-line JSON without re-serializing values", () => {
		expect(
			formatJsonForDisplay('{"id":12345678901234567890,"tags":["a,b","c\\"}"],"empty":{},"n":1.0}'),
		).toBe(
			[
				"{",
				'  "id": 12345678901234567890,',
				'  "tags": [',
				'    "a,b",',
				'    "c\\"}"',
				"  ],",
				'  "empty": {},',
				'  "n": 1.0',
				"}",
			].join("\n"),
		);
	});

	it("leaves multi-line and invalid input as written", () => {
		const authored = '{\n"a": 1 }';
		expect(formatJsonForDisplay(authored)).toBe(authored);
		expect(formatJsonForDisplay("{ nope }")).toBe("{ nope }");
	});
});

describe("fenceBareJson", () => {
	it("fences a raw JSON payload pasted after prose", () => {
		expect(fenceBareJson('olha esse payload:\n{"a": 1,\n "b": [2]}\nfaz sentido?')).toBe(
			['olha esse payload:', "", "```json", '{"a": 1,', ' "b": [2]}', "```", "", "faz sentido?"].join(
				"\n",
			),
		);
		expect(fenceBareJson('{"a":{"b":1}}')).toBe('```json\n{"a":{"b":1}}\n```');
	});

	it("leaves prose, fenced code and bracketed text alone", () => {
		for (const text of [
			"sem json aqui",
			"[ ] tarefa\n[x] feita\n[1]",
			"[docs](https://example.com) e mais texto",
			'```\n{"a": 1}\n```',
			"{ chave: valor }",
			'{"a": 1} e texto depois',
		]) {
			expect(fenceBareJson(text)).toBe(text);
		}
	});
});

describe("fence info", () => {
	it("derives the language from the path and reads the start line", () => {
		expect(fenceLanguageForPath("src/lib/math.ts")).toBe("ts");
		expect(fenceLanguageForPath("Makefile")).toBe("");
		expect(fenceStartLine("startLine=12")).toBe(12);
		expect(fenceStartLine("startLine=0")).toBeUndefined();
		expect(fenceStartLine(undefined)).toBeUndefined();
	});
});
