import { describe, expect, it } from "vitest";
import {
	dictationErrorMessage,
	dictationResult,
	dictationText,
	hasOfflineLocale,
	startDictation,
} from "./dictation.ts";

describe("dictation", () => {
	it("shows partial speech live after what was typed, without erasing it", () => {
		let draft = startDictation("Corrija o bug em");
		draft = dictationResult(draft, "o", false);
		expect(dictationText(draft)).toBe("Corrija o bug em o");
		draft = dictationResult(draft, "o arquivo de login", false);
		expect(dictationText(draft)).toBe("Corrija o bug em o arquivo de login");
		draft = dictationResult(draft, " o arquivo de login ", true);
		expect(dictationText(draft)).toBe("Corrija o bug em o arquivo de login");
		// Android's continuous mode: the next utterance starts from scratch.
		draft = dictationResult(draft, "e rode", false);
		expect(dictationText(draft)).toBe("Corrija o bug em o arquivo de login e rode");
		draft = dictationResult(draft, "e rode os testes", true);
		expect(dictationText(draft)).toBe("Corrija o bug em o arquivo de login e rode os testes");
	});

	it("does not add a space after a line break or on an empty field", () => {
		expect(dictationText(dictationResult(startDictation(""), "olá", false))).toBe("olá");
		expect(dictationText(dictationResult(startDictation("item:\n"), "um", false))).toBe("item:\num");
		expect(dictationText(startDictation("nada falado"))).toBe("nada falado");
	});

	it("knows when pt-BR runs offline", () => {
		expect(hasOfflineLocale(["en-US", "pt_BR"])).toBe(true);
		expect(hasOfflineLocale(["pt-PT"])).toBe(false);
	});

	it("explains errors in Portuguese and stays quiet when the person stopped", () => {
		expect(dictationErrorMessage("aborted")).toBeNull();
		expect(dictationErrorMessage("no-speech")).toBeNull();
		expect(dictationErrorMessage("not-allowed")).toMatch(/permissão/);
		expect(dictationErrorMessage("whatever")).toBe("O ditado parou com um erro.");
	});
});
