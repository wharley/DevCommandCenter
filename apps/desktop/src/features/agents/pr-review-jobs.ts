import { useSyncExternalStore } from "react";

/**
 * Pull request reviews the built-in reviewer runs from the hub. A PR of
 * someone else has no task or session to hold the work, so the job lives
 * here, outside any screen: it keeps running and keeps its result when the
 * person leaves the PR. In memory only; quitting the app drops it.
 */
export type PrReviewJob = {
	prId: string;
	label: string;
	title: string;
	status: "running" | "done" | "failed";
	/** The agent's raw answer; the PR screen parses it against the patches. */
	response: string | null;
	error: string | null;
	finishedAt: number | null;
	/** The person has been on the PR's code tab since it finished. */
	seen: boolean;
};

let jobs: PrReviewJob[] = [];
const listeners = new Set<() => void>();

function publish(next: PrReviewJob[]) {
	jobs = next;
	for (const listener of listeners) {
		listener();
	}
}

function patch(prId: string, change: Partial<PrReviewJob>) {
	publish(jobs.map((job) => (job.prId === prId ? { ...job, ...change } : job)));
}

/** Starts a review unless one is already running for that PR. */
export function startPrReviewJob(
	pr: { prId: string; label: string; title: string },
	run: () => Promise<string>,
): void {
	if (jobs.some((job) => job.prId === pr.prId && job.status === "running")) {
		return;
	}
	publish([
		{ ...pr, status: "running", response: null, error: null, finishedAt: null, seen: false },
		...jobs.filter((job) => job.prId !== pr.prId),
	]);
	run().then(
		(response) => patch(pr.prId, { status: "done", response, finishedAt: Date.now() }),
		(error: unknown) =>
			patch(pr.prId, {
				status: "failed",
				error: error instanceof Error ? error.message : String(error),
				finishedAt: Date.now(),
			}),
	);
}

export function markPrReviewJobSeen(prId: string): void {
	if (jobs.some((job) => job.prId === prId && job.status !== "running" && !job.seen)) {
		patch(prId, { seen: true });
	}
}

/** Forgets a job: the review was submitted or the person discarded it. */
export function dismissPrReviewJob(prId: string): void {
	if (jobs.some((job) => job.prId === prId)) {
		publish(jobs.filter((job) => job.prId !== prId));
	}
}

export function usePrReviewJobs(): PrReviewJob[] {
	return useSyncExternalStore(
		(listener) => {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		() => jobs,
		() => jobs,
	);
}

/** Test seam: start each test from an empty store. */
export function resetPrReviewJobsForTest(): void {
	jobs = [];
}

export function prReviewJobsSnapshot(): PrReviewJob[] {
	return jobs;
}

// ---- Opening a PR from elsewhere (the agent's page) -------------------------

const OPEN_PULL_REQUEST_EVENT = "dcc:open-pull-request";
let pendingOpenPrId: string | null = null;

/** Asks the app to show the hub on this PR's code tab. */
export function requestOpenPullRequest(prId: string): void {
	pendingOpenPrId = prId;
	window.dispatchEvent(new CustomEvent<string>(OPEN_PULL_REQUEST_EVENT, { detail: prId }));
}

/** The hub may mount after the request; it takes the pending one when it does. */
export function takePendingOpenPullRequest(): string | null {
	const prId = pendingOpenPrId;
	pendingOpenPrId = null;
	return prId;
}

export function subscribeOpenPullRequest(listener: (prId: string) => void): () => void {
	const handler = (event: Event) => listener((event as CustomEvent<string>).detail);
	window.addEventListener(OPEN_PULL_REQUEST_EVENT, handler);
	return () => window.removeEventListener(OPEN_PULL_REQUEST_EVENT, handler);
}
