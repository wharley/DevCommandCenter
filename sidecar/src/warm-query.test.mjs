import assert from "node:assert/strict";
import test from "node:test";

import {
	createInputChannel,
	createQueryReader,
	createWarmSlot,
	discardBuffered,
	readTurn,
	warmKey,
} from "./warm-query.mjs";

// A query whose output the test feeds, like the SDK's.
function fakeQuery() {
	const output = createInputChannel();
	let closed = false;
	return {
		emit: (message) => output.push(message),
		finish: () => output.end(),
		close() {
			closed = true;
			output.end();
		},
		get closed() {
			return closed;
		},
		[Symbol.asyncIterator]: () => output.stream[Symbol.asyncIterator](),
	};
}

test("a channel delivers messages in order and ends when told", async () => {
	const channel = createInputChannel();
	const iterator = channel.stream[Symbol.asyncIterator]();
	const first = iterator.next();
	assert.equal(channel.push("a"), true);
	channel.push("b");
	assert.deepEqual(await first, { done: false, value: "a" });
	assert.deepEqual(await iterator.next(), { done: false, value: "b" });
	channel.end();
	assert.equal(channel.push("c"), false, "nothing after the end");
	assert.deepEqual(await iterator.next(), { done: true, value: undefined });
});

test("a read left waiting is the next turn's first read", async () => {
	const query = fakeQuery();
	const reader = createQueryReader(query);
	const waiting = reader.next();
	assert.equal(reader.next(), waiting, "the same promise until taken");
	query.emit("late");
	assert.deepEqual(await reader.next(), { done: false, value: "late" });
	reader.taken();
	query.emit("next");
	assert.deepEqual(await reader.next(), { done: false, value: "next" });
});

test("output from before the turn is dropped, and a finished query is told", async () => {
	const query = fakeQuery();
	const reader = createQueryReader(query);
	query.emit({ type: "system", subtype: "session_state_changed", state: "idle" });
	query.emit({ type: "system", subtype: "notice" });
	assert.deepEqual(await discardBuffered(reader, { settleMs: 5 }), {
		ended: false,
		discarded: 2,
	});
	// What comes after is the turn's own.
	query.emit("answer");
	assert.deepEqual(await reader.next(), { done: false, value: "answer" });
	reader.taken();
	query.finish();
	assert.equal((await discardBuffered(reader, { settleMs: 5 })).ended, true);
});

test("the key changes with what a query is started with", () => {
	const base = { toolInstructions: "be brief", effort: "high" };
	const mcp = { servers: { dcc: { url: "http://127.0.0.1:1" } } };
	const key = warmKey(base, mcp, { DCC_MODEL: "opus" });
	assert.equal(key, warmKey({ ...base, prompt: "another message" }, mcp, { DCC_MODEL: "opus" }));
	assert.equal(key, warmKey({ ...base, keepWarm: true }, mcp, { DCC_MODEL: "opus" }));
	for (const [payload, servers, env] of [
		[{ ...base, toolInstructions: "be thorough" }, mcp, { DCC_MODEL: "opus" }],
		[{ ...base, effort: "low" }, mcp, { DCC_MODEL: "opus" }],
		[{ ...base, planMode: true }, mcp, { DCC_MODEL: "opus" }],
		[{ ...base, fastMode: true }, mcp, { DCC_MODEL: "opus" }],
		[{ ...base, approvalPolicy: "never" }, mcp, { DCC_MODEL: "opus" }],
		[base, { servers: {} }, { DCC_MODEL: "opus" }],
		[base, mcp, { DCC_MODEL: "sonnet" }],
	]) {
		assert.notEqual(key, warmKey(payload, servers, env));
	}
});

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
		pending: () => [...timers.values()].map((timer) => timer.milliseconds),
		fire() {
			const due = [...timers.values()];
			timers.clear();
			for (const timer of due) timer.callback();
		},
	};
}

function entry(key) {
	const query = fakeQuery();
	return { key, query, channel: createInputChannel(), reader: createQueryReader(query) };
}

test("the slot hands back the query for the same key and closes any other", () => {
	const timers = manualTimers();
	const slot = createWarmSlot({ idleMs: 1000, ...timers });
	assert.equal(slot.take("k"), null);

	const first = entry("k");
	slot.keep(first);
	assert.deepEqual(timers.pending(), [1000]);
	assert.equal(slot.take("k"), first);
	assert.deepEqual(timers.pending(), [], "in use: no idle clock");
	assert.equal(first.query.closed, false);
	assert.equal(slot.holds(), false);

	// Other options: the kept one is closed, the turn starts fresh.
	slot.keep(first);
	assert.equal(slot.take("other"), null);
	assert.equal(first.query.closed, true);
	assert.equal(first.channel.push("x"), false, "its prompt ended too");
});

test("a kept query is closed when unused for too long, or replaced", () => {
	const timers = manualTimers();
	const slot = createWarmSlot({ idleMs: 1000, ...timers });
	const first = entry("k");
	slot.keep(first);
	timers.fire();
	assert.equal(first.query.closed, true);
	assert.equal(slot.take("k"), null);

	const second = entry("k");
	const third = entry("k");
	slot.keep(second);
	slot.keep(third);
	assert.equal(second.query.closed, true, "one at a time");
	slot.drop();
	assert.equal(third.query.closed, true);
	assert.deepEqual(timers.pending(), []);
});

test("a turn reads until it is over and leaves the rest for the next", async () => {
	const query = fakeQuery();
	const reader = createQueryReader(query);
	let turnOver;
	const over = new Promise((resolve) => {
		turnOver = resolve;
	});
	const seen = [];
	const reading = readTurn(reader, over, (message) => {
		seen.push(message);
		if (message === "result") turnOver();
	});
	query.emit("text");
	query.emit("result");
	assert.deepEqual(await reading, { ended: false });
	assert.deepEqual(seen, ["text", "result"]);
	assert.equal(query.closed, false, "the query stays open");

	// The next turn, on the same reader.
	let secondOver;
	const second = readTurn(
		reader,
		new Promise((resolve) => {
			secondOver = resolve;
		}),
		(message) => {
			seen.push(message);
			if (message === "result 2") secondOver();
		},
	);
	query.emit("text 2");
	query.emit("result 2");
	assert.deepEqual(await second, { ended: false });
	assert.deepEqual(seen, ["text", "result", "text 2", "result 2"]);

	// A query that finishes under a turn cannot be kept.
	const third = readTurn(reader, new Promise(() => {}), () => {});
	query.finish();
	assert.deepEqual(await third, { ended: true });
});
