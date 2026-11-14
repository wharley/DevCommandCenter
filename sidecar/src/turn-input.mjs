import { userMessage } from "./mcp-readiness.mjs";
import { createInputChannel } from "./warm-query.mjs";

// Asks Claude Code to report `system/session_state_changed`. Its `idle` is the
// authoritative turn-over signal: it fires only after queued messages ran.
export const SESSION_STATE_ENV = { CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS: "1" };

// A steer written just before Claude went idle can still be in its stdin.
// Claude starts it as a follow-up (`running`) almost at once; past this window
// it was already folded into the turn and input can end.
const STEER_SETTLE_MS = 1_000;
// Bound for a Claude that never reports `idle` after a steered turn's result.
const IDLE_FALLBACK_MS = 15_000;

/**
 * The prompt the SDK reads for one DCC turn. It stays open while Claude works
 * so steering can add user messages, and ends when Claude reports the turn
 * over. Without steering it ends at the `result`, as a one-message prompt did.
 * Steering is offered only once Claude has reported that it is running, which
 * proves it will report `idle` and keeps a refused native resume (which fails
 * before that) from swallowing guidance.
 *
 * With a `channel` the prompt belongs to a query kept between turns
 * (`warm-query.mjs`): the turn writes to it and leaves it open at its end,
 * and `over` says when the turn is over instead of the prompt ending.
 */
export function createTurnInput({
	settleMs = STEER_SETTLE_MS,
	idleFallbackMs = IDLE_FALLBACK_MS,
	setTimer = setTimeout,
	clearTimer = clearTimeout,
	channel = null,
} = {}) {
	// The turn's own prompt, ended with it, unless a kept query's was given.
	const sink = channel ?? createInputChannel();
	let turnOver;
	const over = new Promise((resolve) => {
		turnOver = resolve;
	});
	let started = false;
	let closed = false;
	let running = false;
	let resultSinceRunning = false;
	let steers = 0;
	let endTimer = null;

	const cancelEnd = () => {
		if (endTimer !== null) {
			clearTimer(endTimer);
			endTimer = null;
		}
	};
	const close = () => {
		cancelEnd();
		if (!closed) {
			closed = true;
			if (!channel) {
				sink.end();
			}
			turnOver();
		}
	};
	const endAfter = (milliseconds) => {
		cancelEnd();
		endTimer = setTimer(close, milliseconds);
	};

	return {
		stream: sink.stream,
		/** Resolves when the turn is over (the prompt's end, without a channel). */
		over,
		start(prompt, imageBlocks = []) {
			if (started || closed) {
				return;
			}
			started = true;
			sink.push(userMessage(prompt, imageBlocks));
		},
		canSteer() {
			return started && !closed && running && !resultSinceRunning;
		},
		/** Adds guidance to the running turn; false when the window has closed. */
		steer(prompt, imageBlocks = []) {
			if (!this.canSteer()) {
				return false;
			}
			steers += 1;
			// `next` folds it in at Claude's next tool boundary; `now` would abort.
			sink.push({ ...userMessage(prompt, imageBlocks), priority: "next" });
			return true;
		},
		/** Tracks the turn from Claude's output; true for messages only DCC's sidecar reads. */
		observe(message) {
			if (message?.type === "system" && message.subtype === "session_state_changed") {
				if (message.state === "running" || message.state === "requires_action") {
					if (message.state === "running") {
						resultSinceRunning = false;
					}
					running = true;
					cancelEnd();
				} else if (message.state === "idle") {
					running = false;
					if (resultSinceRunning && !closed) {
						endAfter(settleMs);
					}
				}
				return true;
			}
			if (message?.type === "result") {
				resultSinceRunning = true;
				if (steers === 0) {
					close();
				} else if (!closed) {
					endAfter(idleFallbackMs);
				}
			}
			return false;
		},
		close,
	};
}
