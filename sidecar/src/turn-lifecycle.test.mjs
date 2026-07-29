import assert from "node:assert/strict";
import test from "node:test";

import { finishTurn } from "./turn-lifecycle.mjs";

test("releases the active turn before publishing its terminal result", () => {
	const resolutions = [];
	const activeTurn = Promise.resolve();
	const state = {
		running: true,
		activeTurnPromise: activeTurn,
		pendingUserInputs: new Map([
			["user-input", { resolve: (answers) => resolutions.push(answers) }],
		]),
		pendingPermissions: new Map([
			["permission", { resolve: (behavior) => resolutions.push(behavior) }],
		]),
	};
	const terminalResult = { type: "result", is_error: false, result: "done" };
	const emitted = [];

	finishTurn(state, terminalResult, (message) => {
		assert.equal(state.running, false);
		assert.equal(state.activeTurnPromise, null);
		assert.equal(state.pendingUserInputs.size, 0);
		assert.equal(state.pendingPermissions.size, 0);
		emitted.push(message);
	});

	assert.deepEqual(resolutions, [[], "deny"]);
	assert.deepEqual(emitted, [terminalResult]);
});
