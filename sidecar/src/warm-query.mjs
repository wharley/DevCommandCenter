// Keeping Claude up between the turns of one conversation. A DCC turn used
// to be one SDK query: a new Claude Code process that resumed the
// conversation and attached the MCP servers again, which is most of the wait
// before the first sign of an answer. For a conversation DCC marks
// `keepWarm` (an agent's own, in the team room) the query stays open after a
// turn ends: its prompt is a channel the next turn writes to, and its output
// is read from where the last turn stopped. It is dropped when the next turn
// needs other options, when it sat unused for a while, or when anything
// about it went wrong — the turn then starts a fresh query, as before.

// A warm query unused this long is closed: it is a whole Claude process.
export const WARM_IDLE_MS = 10 * 60_000;
// How long output already produced gets to show up before a reused query
// takes the next message.
const STALE_SETTLE_MS = 25;

/** The prompt of a query that outlives its turns: user messages pushed in order. */
export function createInputChannel() {
	const pending = [];
	let wake = null;
	let ended = false;
	const notify = () => {
		const resolve = wake;
		wake = null;
		resolve?.();
	};
	async function* drain() {
		while (true) {
			if (pending.length > 0) {
				yield pending.shift();
				continue;
			}
			if (ended) {
				return;
			}
			await new Promise((resolve) => {
				wake = resolve;
			});
		}
	}
	return {
		stream: drain(),
		push(message) {
			if (ended) {
				return false;
			}
			pending.push(message);
			notify();
			return true;
		},
		end() {
			if (!ended) {
				ended = true;
				notify();
			}
		},
	};
}

/**
 * Reads a query one message at a time without ending it. `next` hands out
 * the same promise until `taken`, so a read left waiting when a turn ends is
 * the first read of the next one and no message is lost between them.
 */
export function createQueryReader(query) {
	const iterator = query[Symbol.asyncIterator]();
	let pending = null;
	return {
		next() {
			pending ??= iterator.next();
			return pending;
		},
		taken() {
			pending = null;
		},
	};
}

const QUIET = Symbol("quiet");
const sleep = (milliseconds) =>
	new Promise((resolve) => setTimeout(resolve, milliseconds));

/**
 * Drops what a kept query produced after its last turn ended (its `idle`
 * report, a late notice) so the next turn starts on its own output.
 * `ended` when the query finished meanwhile: it cannot be reused.
 */
export async function discardBuffered(
	reader,
	{ settleMs = STALE_SETTLE_MS, wait = sleep } = {},
) {
	let discarded = 0;
	while (true) {
		let step;
		try {
			step = await Promise.race([
				reader.next(),
				wait(settleMs).then(() => QUIET),
			]);
		} catch {
			return { ended: true, discarded };
		}
		if (step === QUIET) {
			return { ended: false, discarded };
		}
		reader.taken();
		if (step.done) {
			return { ended: true, discarded };
		}
		discarded += 1;
	}
}

/**
 * What a query was started with, as far as a later turn may differ: the
 * system prompt's parts, the approval mode, the effort, the MCP servers and
 * the model. Another key means another query.
 */
export function warmKey(payload, mcpProjection, env = {}) {
	return JSON.stringify([
		typeof payload?.toolInstructions === "string" ? payload.toolInstructions : null,
		payload?.fastMode === true,
		payload?.planMode === true,
		payload?.approvalPolicy ?? null,
		typeof payload?.effort === "string" ? payload.effort : null,
		mcpProjection ?? null,
		env.DCC_MODEL ?? null,
		env.DCC_ADDITIONAL_DIRECTORIES ?? null,
	]);
}

const OVER = Symbol("over");

/**
 * Hands the query's messages to `handle` until the turn is over (`over`
 * resolves) or the query finishes (`ended`: it cannot be kept).
 */
export async function readTurn(reader, over, handle) {
	const turnOver = over.then(() => OVER);
	while (true) {
		const step = await Promise.race([reader.next(), turnOver]);
		if (step === OVER) {
			return { ended: false };
		}
		reader.taken();
		if (step.done) {
			return { ended: true };
		}
		handle(step.value);
	}
}

function closeEntry(entry) {
	entry.channel.end();
	try {
		entry.query.close();
	} catch {
		// Already gone.
	}
}

/** The one query a sidecar keeps between turns, if any. */
export function createWarmSlot({
	idleMs = WARM_IDLE_MS,
	setTimer = setTimeout,
	clearTimer = clearTimeout,
} = {}) {
	let entry = null;
	let timer = null;
	const disarm = () => {
		if (timer !== null) {
			clearTimer(timer);
			timer = null;
		}
	};
	const drop = () => {
		disarm();
		const kept = entry;
		entry = null;
		if (kept) {
			closeEntry(kept);
		}
	};
	return {
		/** The kept query when it was started with `key`; any other is closed. */
		take(key) {
			disarm();
			if (!entry) {
				return null;
			}
			if (entry.key !== key) {
				drop();
				return null;
			}
			const kept = entry;
			entry = null;
			return kept;
		},
		/** Keeps `{ key, query, channel, reader }` for the next turn. */
		keep(next) {
			drop();
			entry = next;
			timer = setTimer(drop, idleMs);
		},
		drop,
		holds() {
			return entry !== null;
		},
	};
}

export { closeEntry as closeWarmEntry };
