import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { DESKTOP_PT_BR, t, toolStepParts } from "./labels.ts";

function lookup(tree: unknown, key: string): unknown {
	return key.split(".").reduce<unknown>((node, part) => (node as Record<string, unknown> | undefined)?.[part], tree);
}

describe("labels", () => {
	it("match the app's pt-BR texts key by key", () => {
		const desktop = JSON.parse(readFileSync(new URL("../../../apps/native/locales/pt-BR/common.json", import.meta.url), "utf8"));
		for (const [key, text] of Object.entries(DESKTOP_PT_BR)) {
			expect(lookup(desktop, key), key).toBe(text);
		}
	});

	it("interpolates and picks plurals", () => {
		expect(t("conversation.activity.turn.steps", { count: 1 })).toBe("1 passo");
		expect(t("conversation.activity.turn.steps", { count: 3 })).toBe("3 passos");
		expect(t("composer.approval.managedHint", { provider: "Cursor" })).toBe(
			"As permissões são gerenciadas diretamente pelo Cursor.",
		);
	});

	it("label tool steps with a verb and their target", () => {
		expect(toolStepParts("Bash", { command: "yarn test\n--watch" }, true)).toEqual({ verb: "Rodando", target: "yarn test" });
		expect(toolStepParts("Edit", { file: "/repo/src/app/main.ts" }, false)).toEqual({ verb: "Editou", target: "app/main.ts" });
		expect(toolStepParts("mcp__github__create_issue", {}, false)).toEqual({ verb: "Usou", target: "github · create_issue" });
		expect(toolStepParts("Read", null, false)).toEqual({ verb: "Leu", target: null });
	});
});
