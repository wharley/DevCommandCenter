// `dcc-claude-sidecar device --udid <udid> --socket <path>`: one booted iOS
// Simulator, for the app's Device panel and the agent's device tools.
//
// The screen and the input come from serve-sim's native addon
// (`serve-sim-native.node`, Apache-2.0, pinned and signed with the app, next
// to this executable): the simulator's framebuffer read zero-copy and encoded
// to H.264, touches and keys sent as the Simulator's own HID messages. Only
// the addon is used, not serve-sim's web server (it has a shell-exec route).
//
// The app listens on a Unix socket and this process connects to it: the
// addon's Swift code prints to stdout, so the stream cannot share it.
// To the app, chunks in serve-sim's own envelope (u32 big-endian length of
// tag + payload, a tag byte, the payload): 0x01 the avcC decoder description,
// 0x02 a keyframe, 0x03 a delta, 0x05 a JSON reply, 0x06 a JSON status.
// From the app, one JSON command per line (`parseCommand`). The process ends
// when the socket closes, so a crash here never takes the app down and the
// app's going away never leaves the capture running.

import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import net from "node:net";
import { dirname, join } from "node:path";

export const TAG = {
	description: 0x01,
	keyframe: 0x02,
	delta: 0x03,
	reply: 0x05,
	status: 0x06,
};

const CODEC_AVCC = 1;
const FLAG_DESCRIPTION = 1;
const FLAG_KEYFRAME = 2;
/** Past this many unread bytes, deltas wait for the next keyframe. */
const BACKLOG_LIMIT = 8 * 1024 * 1024;
const TOUCH_PHASES = new Set(["begin", "move", "end"]);
const KEY_PHASES = new Set(["down", "up"]);

/** One chunk as the app reads it. */
export function envelope(tag, payload) {
	const body = Buffer.isBuffer(payload) ? payload : Buffer.from(payload);
	const out = Buffer.allocUnsafe(5 + body.length);
	out.writeUInt32BE(body.length + 1, 0);
	out.writeUInt8(tag, 4);
	body.copy(out, 5);
	return out;
}

/** The tag for an encoded frame from the addon's flags. */
export function frameTag(flags) {
	if (flags & FLAG_DESCRIPTION) return TAG.description;
	return flags & FLAG_KEYFRAME ? TAG.keyframe : TAG.delta;
}

function unit(value) {
	return typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= 1;
}

/**
 * A command line from the app, checked before it reaches the addon (a bad
 * value thrown mid-gesture leaves a finger stuck on the simulator).
 * Coordinates are 0..1 of the screen.
 */
export function parseCommand(line) {
	let value;
	try {
		value = JSON.parse(line);
	} catch {
		return { error: "not JSON" };
	}
	if (!value || typeof value !== "object") return { error: "not an object" };
	const id = Number.isInteger(value.id) ? value.id : null;
	const bad = (reason) => ({ id, error: reason });
	switch (value.op) {
		case "touch":
			if (!TOUCH_PHASES.has(value.phase)) return bad("touch phase");
			if (!unit(value.x) || !unit(value.y)) return bad("touch point");
			return { id, op: "touch", phase: value.phase, x: value.x, y: value.y, edge: value.edge | 0 };
		case "multiTouch":
			if (!TOUCH_PHASES.has(value.phase)) return bad("touch phase");
			if (![value.x1, value.y1, value.x2, value.y2].every(unit)) return bad("touch points");
			return {
				id,
				op: "multiTouch",
				phase: value.phase,
				x1: value.x1,
				y1: value.y1,
				x2: value.x2,
				y2: value.y2,
			};
		case "scroll":
			if (!Number.isFinite(value.dx) || !Number.isFinite(value.dy)) return bad("scroll delta");
			return { id, op: "scroll", dx: value.dx, dy: value.dy, x: unit(value.x) ? value.x : undefined, y: unit(value.y) ? value.y : undefined };
		case "key":
			if (!KEY_PHASES.has(value.phase) || !Number.isInteger(value.usage)) return bad("key");
			return { id, op: "key", phase: value.phase, usage: value.usage };
		case "button":
			if (typeof value.name !== "string" || !/^[a-z-]{1,24}$/.test(value.name)) return bad("button");
			return { id, op: "button", name: value.name };
		case "orientation":
			if (!Number.isInteger(value.value)) return bad("orientation");
			return { id, op: "orientation", value: value.value };
		case "describe":
			return { id, op: "describe" };
		default:
			return bad(`unknown op ${String(value.op)}`);
	}
}

function argument(name) {
	const index = process.argv.indexOf(`--${name}`);
	return index >= 0 ? process.argv[index + 1] : undefined;
}

/** The addon beside the executable (bundle), or `DCC_SERVE_SIM_NATIVE` (dev). */
function addonPath() {
	const candidates = [
		process.env.DCC_SERVE_SIM_NATIVE,
		join(dirname(process.execPath), "serve-sim-native.node"),
	].filter(Boolean);
	const found = candidates.find((path) => existsSync(path));
	if (!found) throw new Error(`serve-sim-native.node not found in ${candidates.join(", ")}`);
	return found;
}

async function run(addon, hid, size, command) {
	const [w, h] = size();
	switch (command.op) {
		case "touch":
			return hid.touch(command.phase, command.x, command.y, w, h, command.edge);
		case "multiTouch":
			return hid.multiTouch(command.phase, command.x1, command.y1, command.x2, command.y2, w, h);
		case "scroll":
			return hid.scroll(command.dx, command.dy, command.x ?? Number.NaN, command.y ?? Number.NaN, w, h);
		case "key":
			return hid.key(command.phase, command.usage);
		case "button":
			return hid.button(command.name);
		case "orientation":
			return hid.orientation(command.value);
		case "describe":
			return JSON.parse(await addon.axDescribe(udidArgument()));
	}
}

function udidArgument() {
	const udid = argument("udid");
	if (!udid || !/^[0-9A-Fa-f-]{36}$/.test(udid)) throw new Error("--udid <simulator udid> is required");
	return udid;
}

export async function runDevice() {
	const udid = udidArgument();
	const socketPath = argument("socket");
	if (!socketPath) throw new Error("--socket <path> is required");
	const addon = createRequire(import.meta.url)(addonPath());
	const socket = net.createConnection({ path: socketPath });
	await new Promise((resolve, reject) => {
		socket.once("connect", resolve);
		socket.once("error", reject);
	});
	const send = (tag, payload) => socket.write(envelope(tag, payload));
	const status = (value) => send(TAG.status, JSON.stringify(value));

	const capture = new addon.SimCapture(udid);
	const hid = new addon.SimHID(udid, "");
	let width = 0;
	let height = 0;
	let waitingForKeyframe = false;
	await capture.start();
	const unsubscribe = await capture.subscribe(CODEC_AVCC, async (data, w, h, flags) => {
		if (w !== width || h !== height) {
			width = w;
			height = h;
			status({ width, height });
		}
		const tag = frameTag(flags);
		if (tag === TAG.delta && (waitingForKeyframe || socket.writableLength > BACKLOG_LIMIT)) {
			waitingForKeyframe = true;
			return;
		}
		if (tag === TAG.keyframe) waitingForKeyframe = false;
		send(tag, Buffer.from(data.buffer, data.byteOffset, data.byteLength));
	});
	status({ started: true });

	let pending = "";
	socket.on("data", (chunk) => {
		pending += chunk.toString("utf8");
		let newline = pending.indexOf("\n");
		while (newline >= 0) {
			const line = pending.slice(0, newline);
			pending = pending.slice(newline + 1);
			newline = pending.indexOf("\n");
			if (!line.trim()) continue;
			const command = parseCommand(line);
			if (command.error) {
				send(TAG.reply, JSON.stringify({ id: command.id ?? null, ok: false, error: command.error }));
				continue;
			}
			run(addon, hid, () => [width, height], command).then(
				(result) => send(TAG.reply, JSON.stringify({ id: command.id, ok: true, result: result ?? null })),
				(error) =>
					send(
						TAG.reply,
						JSON.stringify({ id: command.id, ok: false, error: error instanceof Error ? error.message : String(error) }),
					),
			);
		}
	});

	await new Promise((resolve) => {
		socket.once("close", resolve);
		socket.once("error", resolve);
	});
	await unsubscribe?.();
	await capture.stop();
	await hid.close?.();
}
