import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { classifyToolAction, DESKTOP_PT_BR, t, toolStepParts } from "./labels.ts";

const desktopRoot = new URL("../../../apps/desktop/src/", import.meta.url);

function lookup(tree: unknown, key: string): unknown {
	return key.split(".").reduce<unknown>((node, part) => (node as Record<string, unknown> | undefined)?.[part], tree);
}

describe("labels", () => {
	it("match the desktop's pt-BR texts key by key", () => {
		const desktop = JSON.parse(readFileSync(new URL("i18n/locales/pt-BR/common.json", desktopRoot), "utf8"));
		for (const [key, text] of Object.entries(DESKTOP_PT_BR)) {
			expect(lookup(desktop, key), key).toBe(text);
		}
	});

	it("classify tools exactly like the desktop", async () => {
		const path = new URL("features/panel/message-components/turn-activity.logic.ts", desktopRoot).href;
		const desktop = (await import(/* @vite-ignore */ path)) as { classifyToolAction(action: string): string };
		const actions = [
			"Bash", "shell", "commandExecution", "Edit", "MultiEdit", "Write", "apply_patch", "fileChange",
			"Read", "view", "Grep", "Glob", "LS", "WebSearch", "WebFetch", "TodoWrite", "update_plan",
			"Task", "dcc_delegate_task", "mcp__dcc__dcc_delegate_task", "mcp__github__create_issue", "Weird",
		];
		for (const action of actions) expect(classifyToolAction(action), action).toBe(desktop.classifyToolAction(action));
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
