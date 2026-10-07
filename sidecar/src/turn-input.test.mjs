import assert from "node:assert/strict";
import test from "node:test";

import { createTurnInput } from "./turn-input.mjs";

const RUNNING = { type: "system", subtype: "session_state_changed", state: "running" };
const IDLE = { type: "system", subtype: "session_state_changed", state: "idle" };
const RESULT = { type: "result", subtype: "success", is_error: false, result: "done" };
const IMAGE = { type: "image", source: { type: "base64", media_type: "image/png", data: "AA==" } };

function manualTimers() {
	const timers = new Map();
	let nextId = 1;
	return {
		setTimer(callback, milliseconds) {
			const id = nextId++;
			timers.set(id, { callback, milliseconds });
			return id;
		},
		clearTimer(id) {
			timers.delete(id);
		},
		pending() {
			return [...timers.values()].map((timer) => timer.milliseconds);
		},
		fire() {
			const due = [...timers.values()];
			timers.clear();
			for (const timer of due) timer.callback();
		},
	};
}

async function collect(stream) {
	const messages = [];
	for await (const message of stream) messages.push(message);
	return messages;
}

function text(message) {
	return message.message.content.at(-1).text;
}

test("does not deliver the user prompt until it is started", async () => {
	const input = createTurnInput();
	const iterator = input.stream[Symbol.asyncIterator]();
	let delivered = false;
	const next = iterator.next().then((value) => {
		delivered = true;
		return value;
	});

	await Promise.resolve();
	assert.equal(delivered, false);

	input.start("read remote task");
	assert.deepEqual(await next, {
		done: false,
		value: {
			type: "user",
			session_id: "",
			message: {
				role: "user",
				content: [{ type: "text", text: "read remote task" }],
			},
			parent_tool_use_id: null,
		},
	});
	input.close();
	assert.deepEqual(await iterator.next(), { done: true, value: undefined });
});

test("an unsteered turn ends its input at the result, as a one-message prompt did", async () => {
	const input = createTurnInput();
	input.start("plan");
	const messages = collect(input.stream);
	assert.equal(input.observe({ type: "assistant" }), false);
	assert.equal(input.observe(RESULT), false);
	assert.deepEqual((await messages).map(text), ["plan"]);
});

test("offers steering only while Claude reports the turn running", () => {
	const input = createTurnInput();
	assert.equal(input.steer("too early"), false, "before the prompt is sent");
	input.start("plan");
	assert.equal(input.steer("no state yet"), false, "a Claude that never reports state");
	assert.equal(input.observe(RUNNING), true, "session state is the sidecar's own signal");
	assert.equal(input.canSteer(), true);
	input.observe(RESULT);
	assert.equal(input.steer("after the result"), false);
});

test("a Claude without session state ends input at the result and never steers", async () => {
	const input = createTurnInput();
	input.start("plan");
	const messages = collect(input.stream);
	input.observe({ type: "system", subtype: "init" });
	assert.equal(input.steer("guidance"), false);
	input.observe(RESULT);
	assert.deepEqual((await messages).map(text), ["plan"]);
});

test("steered guidance joins the open prompt with images, at the next tool boundary", async () => {
	const timers = manualTimers();
	const input = createTurnInput(timers);
	input.start("plan");
	const iterator = input.stream[Symbol.asyncIterator]();
	assert.equal(text((await iterator.next()).value), "plan");

	input.observe(RUNNING);
	assert.equal(input.steer("also see @/tmp/shot.png", [IMAGE]), true);
	const steered = (await iterator.next()).value;
	assert.equal(steered.priority, "next");
	assert.deepEqual(steered.message.content, [
		IMAGE,
		{ type: "text", text: "also see @/tmp/shot.png" },
	]);
});

test("a steered turn keeps input open past the result until Claude goes idle", async () => {
	const timers = manualTimers();
	const input = createTurnInput({ ...timers, settleMs: 5, idleFallbackMs: 50 });
	input.start("plan");
	const messages = collect(input.stream);
	input.observe(RUNNING);
	input.steer("guidance");

	input.observe(RESULT);
	assert.deepEqual(timers.pending(), [50], "bounded while waiting for idle");
	input.observe(IDLE);
	assert.deepEqual(timers.pending(), [5], "settles briefly after idle");
	timers.fire();
	assert.deepEqual((await messages).map(text), ["plan", "guidance"]);
});

test("a steer Claude runs as a follow-up keeps the turn open through its result", async () => {
	const timers = manualTimers();
	const input = createTurnInput(timers);
	input.start("plan");
	const messages = collect(input.stream);
	input.observe(RUNNING);
	input.steer("guidance");
	input.observe(RESULT);
	input.observe(IDLE);

	// Claude picked the guidance up after going idle: a follow-up turn.
	input.observe(RUNNING);
	assert.deepEqual(timers.pending(), [], "the follow-up cancels the settle window");
	assert.equal(input.steer("more"), true, "the follow-up can be steered too");
	input.observe(RESULT);
	input.observe(IDLE);
	timers.fire();
	assert.deepEqual((await messages).map(text), ["plan", "guidance", "more"]);
	assert.equal(input.steer("late"), false);
});

test("a permission wait keeps the turn steerable", () => {
	const input = createTurnInput();
	input.start("plan");
	input.observe(RUNNING);
	input.observe({ ...RUNNING, state: "requires_action" });
	assert.equal(input.steer("use the other file"), true);
});

test("closing refuses further guidance", () => {
	const input = createTurnInput();
	input.start("plan");
	input.observe(RUNNING);
	input.close();
	assert.equal(input.steer("after close"), false);
});
