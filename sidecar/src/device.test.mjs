import assert from "node:assert/strict";
import { test } from "node:test";

import { envelope, frameTag, parseCommand, TAG } from "./device.mjs";

test("a chunk is its length, its tag and its payload", () => {
	const chunk = envelope(TAG.keyframe, Buffer.from([9, 8, 7]));
	assert.equal(chunk.readUInt32BE(0), 4, "the length covers the tag");
	assert.equal(chunk[4], TAG.keyframe);
	assert.deepEqual([...chunk.subarray(5)], [9, 8, 7]);
	assert.equal(envelope(TAG.reply, '{"ok":true}').subarray(5).toString(), '{"ok":true}');
});

test("the addon's flags pick the tag", () => {
	assert.equal(frameTag(1), TAG.description);
	assert.equal(frameTag(2), TAG.keyframe);
	assert.equal(frameTag(0), TAG.delta);
});

test("only a well-formed command reaches the simulator", () => {
	assert.deepEqual(parseCommand('{"id":3,"op":"touch","phase":"begin","x":0.5,"y":0.25}'), {
		id: 3,
		op: "touch",
		phase: "begin",
		x: 0.5,
		y: 0.25,
		edge: 0,
	});
	assert.equal(parseCommand('{"id":4,"op":"touch","phase":"tap","x":0.5,"y":0.2}').error, "touch phase");
	assert.equal(parseCommand('{"id":5,"op":"touch","phase":"end","x":996,"y":1245}').error, "touch point");
	assert.equal(parseCommand('{"op":"button","name":"home; rm -rf"}').error, "button");
	assert.equal(parseCommand('{"op":"exec","cmd":"ls"}').error, "unknown op exec");
	assert.equal(parseCommand("nope").error, "not JSON");
	assert.deepEqual(parseCommand('{"id":6,"op":"key","phase":"down","usage":4}'), {
		id: 6,
		op: "key",
		phase: "down",
		usage: 4,
	});
});
