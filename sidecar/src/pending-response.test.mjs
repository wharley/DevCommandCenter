import assert from "node:assert/strict";
import { getEventListeners } from "node:events";
import test from "node:test";
import { waitForPendingResponse } from "./pending-response.mjs";
import { finishTurn } from "./turn-lifecycle.mjs";

test("turn completion releases listeners for unanswered questions and permissions", async () => {
	const controller = new AbortController();
	const state = { pendingUserInputs: new Map(), pendingPermissions: new Map() };
	const questions = waitForPendingResponse(state.pendingUserInputs, "question", controller.signal, []);
	const permission = waitForPendingResponse(state.pendingPermissions, "permission", controller.signal, "deny");
	assert.equal(getEventListeners(controller.signal, "abort").length, 2);
	finishTurn(state, { type: "result" }, () => {});
	assert.equal(getEventListeners(controller.signal, "abort").length, 0);
	assert.deepEqual(await questions, { value: [], aborted: false });
	assert.deepEqual(await permission, { value: "deny", aborted: false });
});

test("a response wins over a later cancellation and settles only once", async () => {
	const controller = new AbortController();
	const pending = new Map();
	const response = waitForPendingResponse(pending, "question", controller.signal, []);
	const request = pending.get("question");
	request.resolve(["answer"]);
	controller.abort();
	request.resolve([]);
	assert.deepEqual(await response, { value: ["answer"], aborted: false });
	assert.equal(pending.size, 0);
	assert.equal(getEventListeners(controller.signal, "abort").length, 0);
});

test("cancelling a question releases the pending entry and marks it aborted", async () => {
	const controller = new AbortController();
	const pending = new Map();
	const response = waitForPendingResponse(pending, "question", controller.signal, []);
	controller.abort();
	assert.deepEqual(await response, { value: [], aborted: true });
	assert.equal(pending.size, 0);
	assert.equal(getEventListeners(controller.signal, "abort").length, 0);
});
