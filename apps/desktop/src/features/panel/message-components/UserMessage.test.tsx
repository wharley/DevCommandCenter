import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { UserMessage } from "./UserMessage";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: { lines?: number }) =>
			options?.lines ? `${key}:${options.lines}` : key,
	}),
}));

describe("UserMessage code blocks", () => {
	let container: HTMLDivElement;
	let root: Root;

	beforeEach(() => {
		(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
		container = document.createElement("div");
		document.body.append(container);
		root = createRoot(container);
	});

	afterEach(() => {
		act(() => root.unmount());
		container.remove();
	});

	async function render(content: string) {
		await act(async () => {
			root.render(<UserMessage content={content} label="You" />);
		});
		await vi.waitFor(() => {
			expect(container.querySelector("[data-code-block]")).not.toBeNull();
		});
	}

	it("renders a file snippet as one highlighted block with its real line numbers", async () => {
		await render(
			["Sobre `src/lib/math.ts`:", "", "```ts startLine=3", "const result = a / b;", "return result;", "```"].join(
				"\n",
			),
		);
		// One surface, one label: no block nested inside another.
		expect(container.querySelectorAll("[data-code-block]")).toHaveLength(1);
		expect(container.querySelectorAll('[data-streamdown="code-block"]')).toHaveLength(0);
		const block = container.querySelector("[data-code-block]")!;
		expect(block.getAttribute("data-language")).toBe("ts");
		const pre = block.querySelector("pre")!;
		expect(pre.hasAttribute("data-numbered")).toBe(true);
		expect(pre.style.counterReset).toBe("dcc-code-line 2");
		await vi.waitFor(() => {
			expect(block.querySelector(".dcc-code-line > span[style]")).not.toBeNull();
		});
		expect(pre.textContent).toBe("const result = a / b;\nreturn result;");
	});

	it("formats raw pasted JSON and keeps long payloads collapsed", async () => {
		const payload = JSON.stringify(
			Object.fromEntries(Array.from({ length: 40 }, (_, index) => [`key${index}`, index])),
		);
		await render(`olha esse payload:\n${payload}`);
		const block = container.querySelector("[data-code-block]")!;
		expect(block.getAttribute("data-language")).toBe("json");
		const pre = block.querySelector("pre")!;
		expect(pre.textContent!.split("\n")).toHaveLength(16);
		expect(pre.textContent).toContain('  "key0": 0,');
		const toggle = block.querySelector<HTMLButtonElement>("button[aria-expanded]")!;
		expect(toggle.textContent).toBe("conversation.message.codeShowAll:42");
		await act(async () => toggle.click());
		expect(pre.textContent!.split("\n")).toHaveLength(42);
	});

	it("shows an unlabelled non-JSON fence as plain code without a language label", async () => {
		await render("```\nplain words\n```");
		const block = container.querySelector("[data-code-block]")!;
		expect(block.hasAttribute("data-language")).toBe(false);
		expect(block.textContent).toContain("plain words");
		expect(block.querySelector("pre")!.hasAttribute("data-numbered")).toBe(false);
	});
});
