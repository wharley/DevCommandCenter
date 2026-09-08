// Compare real buffer implementations using synthetic, non-sensitive events.
// Usage: node scripts/audit-session-buffer.mjs [baseline-ref]
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const baseline = process.argv[2] ?? "7d4fecf";
const root = fileURLToPath(new URL("..", import.meta.url));
const path = "apps/desktop/src/features/sessions/session-live-event-buffer.ts";
const beforeSource = execFileSync("git", ["show", `${baseline}:${path}`], { cwd: root, encoding: "utf8" });
const afterSource = readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
async function load(source) {
	const { outputText } = ts.transpileModule(source, {
		compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
	});
	return (await import(`data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`)).SessionLiveEventBuffer;
}
const versions = [["before", await load(beforeSource)], ["after", await load(afterSource)]];
const delta = (content) => ({ sessionTurnDelta: { session_id: "audit", turn_id: "turn", content } });
console.log(JSON.stringify({ baseline, node: process.version, metric: "JSON length * 2; not heap/RSS" }));
for (const [name, Buffer] of versions) {
	const buffer = new Buffer();
	let peak = 0;
	let minimumCounter = 0;
	for (let index = 0; index < 30; index += 1) {
		buffer.append(delta('\n"\\'.repeat(100_000)));
		const actual = buffer.events().reduce((sum, event) => sum + JSON.stringify(event).length * 2, 0);
		peak = Math.max(peak, actual);
		minimumCounter = Math.min(minimumCounter, buffer.stats()[0]?.bytes ?? 0);
		if (name === "after") {
			assert.equal(buffer.stats()[0]?.bytes ?? 0, actual);
			assert.ok(actual <= 8 * 1024 * 1024);
		}
	}
	console.log(JSON.stringify({ version: name, deltas: 30, charsPerDelta: 300_000, peakSerializedBytes: peak, minimumCounter }));
}

// Account for the CPU tradeoff of stricter accounting without measuring the
// expensive independent JSON oracle above. Alternate order across seven rounds.
const timings = { before: [], after: [] };
const input = delta("ordinary streaming content\n");
for (let round = -1; round < 7; round += 1) {
	for (const [name, Buffer] of round % 2 === 0 ? versions : [...versions].reverse()) {
		const buffer = new Buffer();
		const start = performance.now();
		for (let index = 0; index < 10_000; index += 1) buffer.append(input);
		const elapsed = performance.now() - start;
		assert.equal(buffer.events().length, 1);
		if (round >= 0) timings[name].push(elapsed);
	}
}
for (const [version, samplesMs] of Object.entries(timings)) {
	const medianMs = [...samplesMs].sort((a, b) => a - b)[3];
	console.log(JSON.stringify({ version, smallDeltas: 10_000, medianMs, samplesMs }));
}
