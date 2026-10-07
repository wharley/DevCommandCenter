import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EditFromHereDialog } from "./EditFromHereDialog";
import type { EditFromHerePlan } from "./edit-from-here.logic";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			options && "count" in options ? `${key}:${String(options.count)}` : key,
	}),
}));

function plan(overrides: Partial<EditFromHerePlan> = {}): EditFromHerePlan {
	return {
		status: "ready",
		anchorTurnId: "t2",
		removedTurnIds: ["t2", "t3"],
		keptTurnCount: 1,
		anchorPrompt: "second",
		provider: { mode: "native" },
		files: {
			status: "restorable",
			turnCount: 2,
			fileCount: 2,
			files: [
				{ turnId: "t3", displayPath: "src/b.ts", binary: false, preview: null },
				{ turnId: "t2", displayPath: "src/a.ts", binary: false, preview: null },
			],
		},
		...overrides,
	};
}

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
	(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
	container = document.createElement("div");
	document.body.appendChild(container);
	root = createRoot(container);
});

afterEach(() => {
	act(() => root.unmount());
	container.remove();
	document.body.innerHTML = "";
});

function buttons() {
	return Array.from(document.body.querySelectorAll("button")).map(
		(button) => button.textContent ?? "",
	);
}

function click(label: string) {
	const button = Array.from(document.body.querySelectorAll("button")).find(
		(candidate) => candidate.textContent === label,
	);
	if (!button) throw new Error(`no button ${label}: ${buttons().join(", ")}`);
	act(() => button.click());
}

describe("EditFromHereDialog", () => {
	it("restores files only through its own button and lists what would be restored", () => {
		const onConfirm = vi.fn();
		const onCancel = vi.fn();
		act(() =>
			root.render(
				<EditFromHereDialog
					plan={plan()}
					working={false}
					stopped={null}
					onCancel={onCancel}
					onConfirm={onConfirm}
				/>,
			),
		);
		expect(document.body.textContent).toContain("conversation.editFromHere.contextNative");
		expect(document.body.textContent).toContain("src/a.ts");
		click("conversation.editFromHere.keepFiles");
		expect(onConfirm).toHaveBeenLastCalledWith(false);
		click("conversation.editFromHere.restoreFiles:2");
		expect(onConfirm).toHaveBeenLastCalledWith(true);
		click("conversation.editFromHere.cancel");
		expect(onCancel).toHaveBeenCalledTimes(1);
		expect(onConfirm).toHaveBeenCalledTimes(2);
	});

	it("offers no restore when files cannot be restored, and says the agent continues in a new thread", () => {
		const onConfirm = vi.fn();
		act(() =>
			root.render(
				<EditFromHereDialog
					plan={plan({
						provider: { mode: "new_thread", reason: "provider_without_rewind" },
						files: {
							status: "not_restorable",
							turnId: "t2",
							stage: "prepare",
							reasonCode: "target_result_mismatch",
						},
					})}
					working={false}
					stopped={null}
					onCancel={() => {}}
					onConfirm={onConfirm}
				/>,
			),
		);
		expect(buttons().some((label) => label.includes("restoreFiles"))).toBe(false);
		expect(buttons().some((label) => label.includes("newThreadRestore"))).toBe(false);
		expect(document.body.textContent).toContain("conversation.editFromHere.contextNewThread");
		expect(document.body.textContent).toContain("conversation.editFromHere.filesBlocked");
		click("conversation.editFromHere.newThread");
		expect(onConfirm).toHaveBeenCalledWith(false);
	});

	it("after a stopped restore only offers to close", () => {
		act(() =>
			root.render(
				<EditFromHereDialog
					plan={plan()}
					working={false}
					stopped={{
						kind: "files_stopped",
						restoredTurnCount: 1,
						resultKey: "turnReview.guardedUndo.results.blocked",
						reasonKey: "turnReview.guardedUndo.failureReasons.changed",
					}}
					onCancel={() => {}}
					onConfirm={() => {}}
				/>,
			),
		);
		expect(document.body.textContent).toContain("conversation.editFromHere.stoppedTitle");
		expect(document.body.textContent).toContain("conversation.editFromHere.stoppedRestored:1");
		expect(buttons().filter((label) => label.startsWith("conversation.editFromHere."))).toEqual([
			"conversation.editFromHere.close",
		]);
	});
});
