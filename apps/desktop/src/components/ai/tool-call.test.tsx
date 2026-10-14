import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ToolCallRow, type ToolCallAnnotation } from "./tool-call";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, values?: Record<string, unknown>) =>
			values ? `${key} ${JSON.stringify(values)}` : key,
	}),
}));

let root: Root;
let host: HTMLDivElement;

beforeEach(() => {
	(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
	host = document.createElement("div");
	document.body.append(host);
	root = createRoot(host);
});

afterEach(() => {
	act(() => root.unmount());
	host.remove();
});

function planCall(todos: Array<{ content: string; status: string; activeForm?: string }>) {
	return {
		type: "tool-call",
		id: "todo-1",
		action: "TodoWrite",
		command: null,
		file: null,
		content: "",
		detail: { input: JSON.stringify({ todos }, null, 2), truncated: false },
	} as unknown as ToolCallAnnotation;
}

it("shows where the plan is on the row and the steps as a checklist", () => {
	act(() =>
		root.render(
			<ToolCallRow
				live={false}
				annotation={planCall([
					{ content: "Read", status: "completed" },
					{ content: "Fix", status: "in_progress", activeForm: "Fixing" },
					{ content: "Test", status: "pending" },
				])}
			/>,
		),
	);
	expect(host.querySelector(".dcc-tool-target")?.textContent).toContain(
		'conversation.toolCall.planProgress {"done":1,"total":3,"step":"Fixing"}',
	);
	act(() => host.querySelector<HTMLButtonElement>(".dcc-tool-summary")!.click());
	const steps = [...host.querySelectorAll(".dcc-plan-checklist li")];
	expect(steps.map((step) => step.getAttribute("data-status"))).toEqual([
		"completed",
		"in_progress",
		"pending",
	]);
	expect(steps[1]?.textContent).toBe("Fixing");
	// The raw JSON input gives way to the checklist.
	expect(host.querySelector(".dcc-tool-input")).toBeNull();
});

it("says the plan is done once every step is", () => {
	act(() =>
		root.render(
			<ToolCallRow live={false} annotation={planCall([{ content: "Only", status: "completed" }])} />,
		),
	);
	expect(host.querySelector(".dcc-tool-target")?.textContent).toContain(
		"conversation.toolCall.planDone",
	);
});
