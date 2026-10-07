// DCC persists the Claude SDK session id per DCC session and hands it back in
// DCC_RESUME_SESSION_ID when it restarts this sidecar. The first turn resumes
// that native conversation; if Claude no longer has it (deleted transcript,
// another machine, another worktree path) the turn retries fresh with the
// bounded history snapshot DCC sent as `resumeFallbackContext`, and DCC is
// told to forget the id.
//
// After "edit from here" DCC also hands back DCC_RESUME_SESSION_AT: the
// assistant message the conversation must continue from. That turn forks the
// native conversation at that message, so the rewound turns never come back.
// If the cut cannot be made, the turn starts fresh with the snapshot instead
// of resuming the whole conversation.

const NATIVE_SESSION_ID = /^[A-Za-z0-9_-]{1,256}$/;
const RESUME_REJECTED = /No conversation found with session ID/i;
const STDERR_TAIL_MAX_CHARS = 4096;

export function initialNativeResumeId(env) {
	const value =
		typeof env?.DCC_RESUME_SESSION_ID === "string"
			? env.DCC_RESUME_SESSION_ID.trim()
			: "";
	return NATIVE_SESSION_ID.test(value) ? value : null;
}

export function initialNativeResumeAt(env) {
	const value =
		typeof env?.DCC_RESUME_SESSION_AT === "string"
			? env.DCC_RESUME_SESSION_AT.trim()
			: "";
	return NATIVE_SESSION_ID.test(value) ? value : null;
}

export function nativeResumeQueryOptions(state) {
	if (!state.resumeSessionId) {
		return {};
	}
	if (!state.resumeSessionAt) {
		return { resume: state.resumeSessionId };
	}
	return {
		resume: state.resumeSessionId,
		resumeSessionAt: state.resumeSessionAt,
		forkSession: true,
	};
}

// The fork reported its own id in `system/init`: the cut is in place.
export function adoptNativeSession(state, sessionId) {
	state.resumeSessionId = sessionId;
	state.resumeSessionAt = null;
}

export function createTurnTrace() {
	return { sawInit: false, stderrTail: "" };
}

export function recordTurnStderr(trace, data) {
	const tail = trace.stderrTail + String(data);
	trace.stderrTail = tail.slice(-STDERR_TAIL_MAX_CHARS);
}

function resultMentionsRejection(result) {
	if (!result || typeof result !== "object") {
		return false;
	}
	const texts = [
		...(Array.isArray(result.errors) ? result.errors : []),
		result.result,
	];
	return texts.some((text) => typeof text === "string" && RESUME_REJECTED.test(text));
}

// Claude refuses an unknown resume id before it initializes the session, so
// a failure after `system/init` is never treated as a refusal.
export function isResumeRejection(trace, terminalResult) {
	if (trace.sawInit) {
		return false;
	}
	return RESUME_REJECTED.test(trace.stderrTail) || resultMentionsRejection(terminalResult);
}

// Unknown is not missing: lookup failures let the resume attempt decide.
export async function nativeSessionExists(sessionId, cwd, getSessionInfo) {
	try {
		return (await getSessionInfo(sessionId, { dir: cwd })) !== undefined;
	} catch {
		return true;
	}
}

export function withResumeFallback(payload, fallbackContext) {
	if (!fallbackContext) {
		return payload;
	}
	const existing =
		typeof payload.toolInstructions === "string" ? payload.toolInstructions.trim() : "";
	return {
		...payload,
		toolInstructions: existing ? `${existing}\n\n${fallbackContext}` : fallbackContext,
	};
}

function forgetNativeResume(state, emit, sessionId) {
	state.resumeSessionId = null;
	state.resumeSessionAt = null;
	emit({ type: "dcc_native_resume_rejected", session_id: sessionId });
}

export async function runTurnWithNativeResume(
	payload,
	state,
	runTurn,
	{ emit, getSessionInfo, cwd },
) {
	const { resumeFallbackContext, ...turnPayload } = payload ?? {};
	const fallbackContext =
		typeof resumeFallbackContext === "string" && resumeFallbackContext.trim().length > 0
			? resumeFallbackContext.trim()
			: null;
	const freshPayload = withResumeFallback(turnPayload, fallbackContext);

	if (state.nativeResumePending) {
		state.nativeResumePending = false;
		const persistedId = state.resumeSessionId;
		if (persistedId && !(await nativeSessionExists(persistedId, cwd, getSessionInfo))) {
			forgetNativeResume(state, emit, persistedId);
		}
	}

	if (!state.resumeSessionId) {
		return runTurn(freshPayload, state, createTurnTrace());
	}

	// The native conversation already holds the history; the fallback
	// snapshot would only duplicate it.
	const resumedId = state.resumeSessionId;
	const cutting = Boolean(state.resumeSessionAt);
	const trace = createTurnTrace();
	const terminalResult = await runTurn(turnPayload, state, trace);
	// A cut that never got to `system/init` is not retried: resuming without
	// it would bring the rewound turns back.
	const cutFailed = cutting && !trace.sawInit;
	if (!cutFailed && !isResumeRejection(trace, terminalResult)) {
		return terminalResult;
	}
	forgetNativeResume(state, emit, resumedId);
	return runTurn(freshPayload, state, createTurnTrace());
}
