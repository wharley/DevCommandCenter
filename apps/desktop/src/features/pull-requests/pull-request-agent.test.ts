import { describe, expect, it } from "vitest";
import type { PullRequestHubDetailOutput } from "@dcc/contracts";
import { parseAgentReply, parseAgentReview } from "./pull-request-agent";

describe("pull request agent drafts", () => {
	it("parses fenced reply JSON", () => {
		expect(parseAgentReply('```json\n{"reply":"Looks good now."}\n```')).toBe("Looks good now.");
	});

	it("drops hallucinated inline locations", () => {
		const detail = {
			files: [{ path: "src/a.ts", previousPath: null, status: "modified", additions: 1, deletions: 0, patch: "@@ -1 +1 @@\n-old\n+new", blobUrl: null }],
		} as unknown as PullRequestHubDetailOutput;
		const review = parseAgentReview('{"summary":"Review","comments":[{"path":"src/a.ts","line":1,"side":"right","body":"Valid"},{"path":"src/missing.ts","line":99,"side":"right","body":"Invalid"}]}', detail);
		expect(review.comments).toEqual([{ path: "src/a.ts", line: 1, side: "right", body: "Valid" }]);
	});
});
