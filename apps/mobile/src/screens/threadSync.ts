/**
 * Keeps one thread's history in sync over the channel (`planDelta` in
 * @dcc/mobile-core): the durable `sequence` is the cursor, live events only
 * trigger a delta fetch (debounced), a rewind or a shrinking history
 * refetches everything.
 *
 * What the relay channel adds: the subscription carries `after=<cursor>`, so
 * on every (re)open the desktop first replays what this phone missed as
 * `replay` events (raw, durable) — a reconnect costs no full refetch.
 */
import {
	applyEvents,
	createThreadState,
	deltaPath,
	incomingEventSessionId,
	isRewind,
	maxSequence,
	pendingQuestions,
	planDelta,
	type EventsDelta,
	type QuestionRequest,
	type RawSessionEvent,
	type ThreadState,
} from "@dcc/mobile-core";
import type { Backend, StreamEvent } from "../backend/types";
import { errorMessage } from "../backend/types";

export type ThreadSnapshot = {
	state: ThreadState;
	questions: QuestionRequest[];
	loading: boolean;
	error: string | null;
	lastSynced: number | null;
};

type Mode = "delta" | "full";

export class ThreadSync {
	private readonly backend: Backend;
	private readonly threadId: string;
	private readonly emit: (snapshot: ThreadSnapshot) => void;
	private snapshot: ThreadSnapshot;
	private held: RawSessionEvent[] = [];
	private cursor = 0;
	private running = false;
	private queued: Mode | null = null;
	private pending: Mode | null = null;
	private timer: ReturnType<typeof setTimeout> | null = null;
	private poll: ReturnType<typeof setInterval> | null = null;
	private unsubscribe: (() => void) | null = null;
	private stopped = false;

	constructor(backend: Backend, threadId: string, emit: (snapshot: ThreadSnapshot) => void) {
		this.backend = backend;
		this.threadId = threadId;
		this.emit = emit;
		this.snapshot = { state: createThreadState(), questions: [], loading: true, error: null, lastSynced: null };
	}

	start(): void {
		void this.sync("full").then(() => {
			if (this.stopped) return;
			// Subscribe once the history is in: the path carries the cursor,
			// so the desktop replays only what came after it.
			this.unsubscribe = this.backend.subscribe(
				() =>
					`/api/v1/events/stream?sessionId=${encodeURIComponent(this.threadId)}&after=${this.cursor}`,
				{
					onOpen: (reopened) => {
						if (reopened) this.schedule("delta");
					},
					onEvent: (event) => this.onStreamEvent(event),
				},
			);
		});
		// Safety net for a stream that is silently stuck; one empty delta.
		this.poll = setInterval(() => this.schedule("delta"), 60_000);
	}

	stop(): void {
		this.stopped = true;
		this.unsubscribe?.();
		if (this.timer) clearTimeout(this.timer);
		if (this.poll) clearInterval(this.poll);
	}

	/** After the phone sends something (a turn, an answer): pull it in now. */
	nudge(): void {
		this.schedule("delta");
	}

	current(): ThreadSnapshot {
		return this.snapshot;
	}

	private onStreamEvent(event: StreamEvent) {
		if (event.event === "resync") {
			this.schedule("full");
			return;
		}
		if (event.event === "replay") {
			const raw = event.data as RawSessionEvent;
			if (!raw || typeof raw.sequence !== "number" || raw.sequence <= this.cursor) return;
			if (isRewind(raw)) {
				this.schedule("full");
				return;
			}
			this.apply([raw]);
			return;
		}
		if (incomingEventSessionId(event.data) === this.threadId) this.schedule("delta");
	}

	private apply(events: RawSessionEvent[]) {
		const fresh = events.filter((e) => e.sequence > this.cursor);
		if (fresh.length === 0) return;
		this.cursor = maxSequence(fresh, this.cursor);
		this.held = [...this.held, ...fresh];
		this.publish({
			state: applyEvents(this.snapshot.state, fresh),
			questions: pendingQuestions(this.held),
			error: null,
			lastSynced: Date.now(),
		});
	}

	private replaceAll(events: RawSessionEvent[]) {
		this.held = events;
		this.cursor = maxSequence(events);
		this.publish({
			state: applyEvents(createThreadState(), events),
			questions: pendingQuestions(events),
			error: null,
			lastSynced: Date.now(),
		});
	}

	private publish(patch: Partial<ThreadSnapshot>) {
		if (this.stopped) return;
		this.snapshot = { ...this.snapshot, ...patch };
		this.emit(this.snapshot);
	}

	private async fetchFull() {
		const events = await this.backend.request<RawSessionEvent[]>(
			"GET",
			`/api/v1/sessions/${encodeURIComponent(this.threadId)}/events`,
		);
		if (!this.stopped) this.replaceAll(events);
	}

	private async fetchDelta(): Promise<void> {
		for (;;) {
			const delta = await this.backend.request<EventsDelta>("GET", deltaPath(this.threadId, this.cursor));
			if (this.stopped) return;
			const plan = planDelta(this.cursor, delta);
			if (plan.kind === "reset") return this.fetchFull();
			if (plan.events.length > 0) this.apply(plan.events);
			else this.publish({ error: null, lastSynced: Date.now() });
			if (!plan.more) return;
		}
	}

	private async sync(mode: Mode): Promise<void> {
		if (this.stopped) return;
		if (this.running) {
			this.queued = this.queued === "full" || mode === "full" ? "full" : "delta";
			return;
		}
		this.running = true;
		try {
			await (mode === "full" ? this.fetchFull() : this.fetchDelta());
		} catch (error) {
			this.publish({ error: errorMessage(error, "Falha ao atualizar.") });
		} finally {
			this.running = false;
			this.publish({ loading: false });
			if (this.queued && !this.stopped) {
				const next = this.queued;
				this.queued = null;
				this.schedule(next);
			}
		}
	}

	/** Coalesces bursts; a full refetch requested meanwhile wins over a delta. */
	private schedule(mode: Mode) {
		if (this.stopped) return;
		this.pending = this.pending === "full" || mode === "full" ? "full" : "delta";
		if (this.timer) return;
		this.timer = setTimeout(() => {
			this.timer = null;
			const next = this.pending ?? "delta";
			this.pending = null;
			void this.sync(next);
		}, 150);
	}
}
