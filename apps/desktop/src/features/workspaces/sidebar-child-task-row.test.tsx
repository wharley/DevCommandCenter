import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SidebarChildTaskRow } from "./sidebar-child-task-row";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

describe("SidebarChildTaskRow", () => {
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

	it("says when a task waits on the person and opens its conversation", async () => {
		const onSelect = vi.fn();
		await act(async () => {
			root.render(
				<>
					<SidebarChildTaskRow task={{ id: "t1", title: "Fase 1: relay", state: "working", sessionId: "c1" }} onSelect={onSelect} />
					<SidebarChildTaskRow task={{ id: "t2", title: "Fase 2: push", state: "proposed", sessionId: "p" }} last />
				</>,
			);
		});
		const open = container.querySelector<HTMLButtonElement>('[data-child-task="t1"]');
		const proposed = container.querySelector<HTMLButtonElement>('[data-child-task="t2"]');
		expect(open?.textContent).toBe("Fase 1: relay");
		expect(proposed?.textContent).toBe("Fase 2: pushsidebar.childTask.proposed");
		expect(proposed?.disabled).toBe(true);
		// The guide runs through the first row and stops halfway down the last.
		expect(open?.querySelector("[data-guide]")?.getAttribute("data-guide")).toBe("through");
		expect(proposed?.querySelector("[data-guide]")?.getAttribute("data-guide")).toBe("last");
		await act(async () => open?.click());
		expect(onSelect).toHaveBeenCalledOnce();
	});
});
