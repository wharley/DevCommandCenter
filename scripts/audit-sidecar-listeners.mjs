// Counts retained resources, not RSS: keep the signal alive after each request.
// Usage: node scripts/audit-sidecar-listeners.mjs [baseline-ref]
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { getEventListeners } from "node:events";
import { fileURLToPath } from "node:url";
import { handlePermissionRequest as after } from "../sidecar/src/permission-bridge.mjs";

const baseline = process.argv[2] ?? "7d4fecf";
const source = execFileSync("git", ["show", `${baseline}:sidecar/src/permission-bridge.mjs`], {
	cwd: fileURLToPath(new URL("..", import.meta.url)), encoding: "utf8",
});
const { handlePermissionRequest: before } = await import(
	`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`
);
console.log(JSON.stringify({ baseline, node: process.version }));
for (const [name, handler] of [["before", before], ["after", after]]) {
	const state = { pendingPermissions: new Map() };
	const controller = new AbortController();
	for (let index = 1; index <= 10_000; index += 1) {
		const toolUseID = `request-${index}`;
		const response = handler("Tool", {}, { toolUseID, signal: controller.signal }, state, () => {});
		state.pendingPermissions.get(toolUseID).resolve("allow");
		assert.equal((await response).behavior, "allow");
		if ([100, 1000, 10_000].includes(index)) {
			const retained = getEventListeners(controller.signal, "abort").length;
			console.log(JSON.stringify({ version: name, requests: index, pending: state.pendingPermissions.size, retainedAbortListeners: retained }));
			if (name === "after") assert.equal(retained, 0);
		}
	}
	controller.abort();
}
