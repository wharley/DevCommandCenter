import { afterEach, describe, expect, it, vi } from "vitest";
import {
	dispatchWorkbenchCommand,
	subscribeWorkbenchCommand,
} from "./workbench-command";

describe("workbench commands", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("dispatches typed commands and supports unsubscribe", () => {
		vi.stubGlobal("window", new EventTarget());
		const received: string[] = [];
		const unsubscribe = subscribeWorkbenchCommand((command) => received.push(command));

		dispatchWorkbenchCommand("composer.focus");
		unsubscribe();
		dispatchWorkbenchCommand("terminal.openWorktree");

		expect(received).toEqual(["composer.focus"]);
	});
});
