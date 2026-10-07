import assert from "node:assert/strict";
import test from "node:test";

import {
	createTurnTrace,
	initialNativeResumeId,
	isResumeRejection,
	recordTurnStderr,
	runTurnWithNativeResume,
	withResumeFallback,
} from "./native-resume.mjs";

const NATIVE_ID = "3f1c2b8e-1111-4000-8000-000000000000";
const FALLBACK = "<dcc_reanchor>earlier exchange</dcc_reanchor>";

function harness({ nativeId = NATIVE_ID, sessionInfo = {}, attempts = [] } = {}) {
	const state = {
		resumeSessionId: nativeId,
		nativeResumePending: nativeId !== null,
	};
	const emitted = [];
	const calls = [];
	const runTurn = async (payload, turnState, trace) => {
		calls.push({ payload, resume: turnState.resumeSessionId });
		const attempt = attempts[calls.length - 1] ?? { initId: "fresh-id" };
		if (attempt.stderr) {
			recordTurnStderr(trace, attempt.stderr);
		}
		if (attempt.initId) {
			turnState.resumeSessionId = attempt.initId;
			trace.sawInit = true;
		}
		return attempt.result ?? { type: "result", is_error: false, result: "done" };
	};
	const lookups = [];
	const deps = {
		emit: (message) => emitted.push(message),
		cwd: "/work/tree",
		getSessionInfo: async (id, options) => {
			lookups.push({ id, options });
			if (sessionInfo instanceof Error) {
				throw sessionInfo;
			}
			return sessionInfo ?? undefined;
		},
	};
	return { state, emitted, calls, lookups, runTurn, deps };
}

const payload = {
	type: "input",
	prompt: "next step",
	toolInstructions: "scope",
	resumeFallbackContext: FALLBACK,
};

test("reads only a well-formed persisted resume id from the environment", () => {
	assert.equal(initialNativeResumeId({ DCC_RESUME_SESSION_ID: ` ${NATIVE_ID} ` }), NATIVE_ID);
	assert.equal(initialNativeResumeId({ DCC_RESUME_SESSION_ID: "" }), null);
	assert.equal(initialNativeResumeId({ DCC_RESUME_SESSION_ID: "../x" }), null);
	assert.equal(initialNativeResumeId({}), null);
});

test("resumes the persisted native session without the fallback snapshot", async () => {
	const h = harness({ attempts: [{ initId: NATIVE_ID }] });
	const result = await runTurnWithNativeResume(payload, h.state, h.runTurn, h.deps);

	assert.equal(result.is_error, false);
	assert.deepEqual(h.lookups, [{ id: NATIVE_ID, options: { dir: "/work/tree" } }]);
	assert.equal(h.calls.length, 1);
	assert.equal(h.calls[0].resume, NATIVE_ID);
	assert.equal(h.calls[0].payload.toolInstructions, "scope");
	assert.equal("resumeFallbackContext" in h.calls[0].payload, false);
	assert.deepEqual(h.emitted, []);
	assert.equal(h.state.nativeResumePending, false);
	assert.equal(h.state.resumeSessionId, NATIVE_ID);
});

test("a missing native session starts fresh with the fallback and forgets the id", async () => {
	const h = harness({ sessionInfo: null, attempts: [{ initId: "fresh-id" }] });
	await runTurnWithNativeResume(payload, h.state, h.runTurn, h.deps);

	assert.deepEqual(h.emitted, [
		{ type: "dcc_native_resume_rejected", session_id: NATIVE_ID },
	]);
	assert.equal(h.calls.length, 1);
	assert.equal(h.calls[0].resume, null);
	assert.equal(h.calls[0].payload.toolInstructions, `scope\n\n${FALLBACK}`);
	assert.equal(h.state.resumeSessionId, "fresh-id");
});

test("a resume Claude refuses at runtime retries once, fresh, with the fallback", async () => {
	const h = harness({
		attempts: [
			{
				result: {
					type: "result",
					is_error: true,
					errors: [`No conversation found with session ID: ${NATIVE_ID}`],
				},
			},
			{ initId: "fresh-id" },
		],
	});
	const result = await runTurnWithNativeResume(payload, h.state, h.runTurn, h.deps);

	assert.equal(result.is_error, false);
	assert.equal(h.calls.length, 2);
	assert.equal(h.calls[0].resume, NATIVE_ID);
	assert.equal(h.calls[0].payload.toolInstructions, "scope");
	assert.equal(h.calls[1].resume, null);
	assert.equal(h.calls[1].payload.toolInstructions, `scope\n\n${FALLBACK}`);
	assert.deepEqual(h.emitted, [
		{ type: "dcc_native_resume_rejected", session_id: NATIVE_ID },
	]);
	assert.equal(h.state.resumeSessionId, "fresh-id");
});

test("a refusal seen only on stderr (MCP startup path) also falls back", async () => {
	const h = harness({
		attempts: [
			{
				stderr: `No conversation found with session ID: ${NATIVE_ID}\n`,
				result: { type: "result", is_error: true, result: "DCC MCP attachment failed" },
			},
			{ initId: "fresh-id" },
		],
	});
	await runTurnWithNativeResume(payload, h.state, h.runTurn, h.deps);
	assert.equal(h.calls.length, 2);
	assert.equal(h.emitted.length, 1);
});

test("other failures keep the native id and do not retry", async () => {
	const failure = { type: "result", is_error: true, result: "rate limited" };
	const h = harness({ attempts: [{ initId: NATIVE_ID, result: failure }] });
	const result = await runTurnWithNativeResume(payload, h.state, h.runTurn, h.deps);

	assert.equal(result, failure);
	assert.equal(h.calls.length, 1);
	assert.deepEqual(h.emitted, []);
	assert.equal(h.state.resumeSessionId, NATIVE_ID);
});

test("an unavailable session lookup still attempts the resume", async () => {
	const h = harness({ sessionInfo: new Error("unreadable"), attempts: [{ initId: NATIVE_ID }] });
	await runTurnWithNativeResume(payload, h.state, h.runTurn, h.deps);
	assert.equal(h.calls[0].resume, NATIVE_ID);
	assert.deepEqual(h.emitted, []);
});

test("later turns are not re-verified and a fresh runtime uses the fallback", async () => {
	const live = harness({ attempts: [{ initId: NATIVE_ID }, { initId: NATIVE_ID }] });
	await runTurnWithNativeResume(payload, live.state, live.runTurn, live.deps);
	await runTurnWithNativeResume({ type: "input", prompt: "again" }, live.state, live.runTurn, live.deps);
	assert.equal(live.lookups.length, 1);
	assert.equal(live.calls[1].resume, NATIVE_ID);

	const fresh = harness({ nativeId: null });
	await runTurnWithNativeResume(payload, fresh.state, fresh.runTurn, fresh.deps);
	assert.equal(fresh.lookups.length, 0);
	assert.equal(fresh.calls[0].payload.toolInstructions, `scope\n\n${FALLBACK}`);
});

test("refusal detection ignores failures after the session initialized", () => {
	const trace = createTurnTrace();
	const refused = {
		type: "result",
		is_error: true,
		errors: ["No conversation found with session ID: x"],
	};
	assert.equal(isResumeRejection(trace, refused), true);
	trace.sawInit = true;
	assert.equal(isResumeRejection(trace, refused), false);
});

test("stderr capture stays bounded", () => {
	const trace = createTurnTrace();
	recordTurnStderr(trace, "x".repeat(10_000));
	recordTurnStderr(trace, "No conversation found with session ID: y");
	assert.ok(trace.stderrTail.length <= 4096);
	assert.equal(isResumeRejection(trace, null), true);
});

test("fallback merges into tool instructions only when present", () => {
	assert.deepEqual(withResumeFallback({ prompt: "p" }, null), { prompt: "p" });
	assert.equal(withResumeFallback({ prompt: "p" }, FALLBACK).toolInstructions, FALLBACK);
});
