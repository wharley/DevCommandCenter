import type { CoreEvent } from "@dcc/contracts";
import { sessionIdForLiveEvent } from "./session-live-event-buffer";

type FrameScheduler = {
	schedule: (callback: () => void) => number;
	cancel: (handle: number) => void;
};

export const MAX_PENDING_SESSION_EVENTS = 256;

const browserFrameScheduler: FrameScheduler = {
	schedule: (callback) => window.requestAnimationFrame(callback),
	cancel: (handle) => window.cancelAnimationFrame(handle),
};

/**
 * Preserves native event order, normally publishing once per animation frame.
 * A full batch publishes eagerly even if frames are suspended. The durable/live
 * buffer may still be updated eagerly.
 */
export class SessionEventFrameBatch {
	private pending: CoreEvent[] = [];
	private scheduledHandle: number | null = null;
	private disposed = false;

	constructor(
		private readonly onFlush: (events: readonly CoreEvent[]) => void,
		private readonly scheduler: FrameScheduler = browserFrameScheduler,
	) {}

	enqueue(event: CoreEvent) {
		if (this.disposed) return;
		this.pending.push(event);
		// WebViews may suspend animation frames while hidden. Apply backpressure
		// by publishing a bounded batch; never drop lifecycle events or deltas.
		if (this.pending.length >= MAX_PENDING_SESSION_EVENTS) {
			this.flush();
			return;
		}
		if (this.scheduledHandle !== null) return;
		this.scheduledHandle = this.scheduler.schedule(() => {
			this.scheduledHandle = null;
			this.flush();
		});
	}

	purgeSession(sessionId: string) {
		this.pending = this.pending.filter(
			(event) => sessionIdForLiveEvent(event) !== sessionId,
		);
	}

	purgeSessions(sessionIds: Iterable<string>) {
		const ids = new Set(sessionIds);
		if (ids.size === 0) return;
		this.pending = this.pending.filter((event) => {
			const sessionId = sessionIdForLiveEvent(event);
			return !sessionId || !ids.has(sessionId);
		});
	}

	flush() {
		if (this.disposed || this.pending.length === 0) return;
		if (this.scheduledHandle !== null) {
			this.scheduler.cancel(this.scheduledHandle);
			this.scheduledHandle = null;
		}
		const events = this.pending;
		this.pending = [];
		this.onFlush(events);
	}

	dispose() {
		if (this.disposed) return;
		this.disposed = true;
		if (this.scheduledHandle !== null) {
			this.scheduler.cancel(this.scheduledHandle);
			this.scheduledHandle = null;
		}
		this.pending = [];
	}
}
