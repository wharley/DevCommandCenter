import { useSyncExternalStore } from "react";

export type ReporterRecapNews = {
	text: string;
	onOpen: () => void;
	onDismiss: () => void;
};

export type ReporterCleanupAlert = {
	totalBytes: number;
	safeCount: number;
	safeBytes: number;
	/** Set while the reporter checks (`total` 0) and then deletes the safe tasks. */
	progress: { done: number; total: number } | null;
};

export type ReporterCleanupOffer = {
	alert: ReporterCleanupAlert;
	onReview: () => void;
	onCleanSafe: () => void;
	onDismiss: () => void;
};

export type ReporterNews = {
	recap: ReporterRecapNews | null;
	cleanup: ReporterCleanupOffer | null;
};

/**
 * The sidebar owns what the reporter knows (the recap window, the completed
 * tasks and their scan); the reporter says it above the composer. This is the
 * line between the two: the sidebar publishes, the composer shows, and says
 * when the reporter has walked over so its sidebar row can show it gone.
 */
let news: ReporterNews = { recap: null, cleanup: null };
/** Composers the reporter currently stands above (a split view can show two). */
let awayCount = 0;
/** The walk over plays once per visit, not on every task switch. */
let walked = false;
const listeners = new Set<() => void>();

function emit() {
	for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}

function publish(next: ReporterNews) {
	news = next;
	// With nothing left to say it goes home; the next visit walks again.
	if (!next.recap && !next.cleanup) walked = false;
	emit();
}

export function publishReporterRecap(recap: ReporterRecapNews | null): void {
	if (recap !== news.recap) publish({ ...news, recap });
}

export function publishReporterCleanup(cleanup: ReporterCleanupOffer | null): void {
	if (cleanup !== news.cleanup) publish({ ...news, cleanup });
}

export function useReporterNews(): ReporterNews {
	return useSyncExternalStore(subscribe, () => news, () => news);
}

/**
 * One thing at a time: the recap first, then the cleanup; a cleanup under way
 * keeps the floor until it ends.
 */
export function reporterSpeech(
	current: ReporterNews,
):
	| { kind: "recap"; recap: ReporterRecapNews }
	| { kind: "cleanup"; cleanup: ReporterCleanupOffer }
	| null {
	if (current.cleanup?.alert.progress) return { kind: "cleanup", cleanup: current.cleanup };
	if (current.recap) return { kind: "recap", recap: current.recap };
	if (current.cleanup) return { kind: "cleanup", cleanup: current.cleanup };
	return null;
}

export function hasReporterWalked(): boolean {
	return walked;
}

export function markReporterWalked(): void {
	walked = true;
}

/** The reporter left its sidebar row for a composer; call the result when it leaves. */
export function reporterSteppedOut(): () => void {
	awayCount += 1;
	emit();
	let back = false;
	return () => {
		if (back) return;
		back = true;
		awayCount -= 1;
		emit();
	};
}

/** True while the reporter stands above a composer instead of in its sidebar row. */
export function useReporterAway(): boolean {
	return useSyncExternalStore(subscribe, () => awayCount > 0, () => awayCount > 0);
}
