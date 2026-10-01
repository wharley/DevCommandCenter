import { beforeEach, describe, expect, it } from "vitest";
import {
	dismissPrReviewJob,
	markPrReviewJobSeen,
	prReviewJobsSnapshot,
	resetPrReviewJobsForTest,
	startPrReviewJob,
} from "./pr-review-jobs";

const pr = { prId: "pr-1", label: "acme/app#12", title: "Add divide" };
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("pull request review jobs", () => {
	beforeEach(resetPrReviewJobsForTest);

	it("keeps the result after the screen that started it is gone", async () => {
		let finish: (value: string) => void = () => {};
		startPrReviewJob(pr, () => new Promise<string>((resolve) => (finish = resolve)));
		expect(prReviewJobsSnapshot()[0]).toMatchObject({ status: "running", seen: false });

		finish('{"summary":"ok","comments":[]}');
		await settle();
		expect(prReviewJobsSnapshot()[0]).toMatchObject({
			status: "done",
			response: '{"summary":"ok","comments":[]}',
			seen: false,
		});
		markPrReviewJobSeen("pr-1");
		expect(prReviewJobsSnapshot()[0]?.seen).toBe(true);
		dismissPrReviewJob("pr-1");
		expect(prReviewJobsSnapshot()).toEqual([]);
	});

	it("does not start a second review of a PR that is being reviewed", async () => {
		let runs = 0;
		const run = () => {
			runs += 1;
			return new Promise<string>(() => {});
		};
		startPrReviewJob(pr, run);
		startPrReviewJob(pr, run);
		expect(runs).toBe(1);
		// A running job cannot be marked as read.
		markPrReviewJobSeen("pr-1");
		expect(prReviewJobsSnapshot()[0]?.seen).toBe(false);
	});

	it("records a failure and lets the review be started again", async () => {
		startPrReviewJob(pr, () => Promise.reject(new Error("provider offline")));
		await settle();
		expect(prReviewJobsSnapshot()[0]).toMatchObject({ status: "failed", error: "provider offline" });
		startPrReviewJob(pr, () => Promise.resolve("{}"));
		await settle();
		expect(prReviewJobsSnapshot()).toHaveLength(1);
		expect(prReviewJobsSnapshot()[0]?.status).toBe("done");
	});
});
