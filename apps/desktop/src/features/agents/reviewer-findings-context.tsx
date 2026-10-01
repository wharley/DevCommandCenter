import { createContext, useContext, useMemo } from "react";
import type { ReviewFinding } from "./review-findings";
import { useReviewerFindings } from "./use-reviewer-findings";

type ReviewerFindings = { workspaceId: string | null; findings: ReviewFinding[] };

const ReviewerFindingsContext = createContext<ReviewerFindings>({
	workspaceId: null,
	findings: [],
});

/** Makes the task's current reviewer findings available to every diff below. */
export function ReviewerFindingsProvider({
	workspaceId,
	children,
}: {
	workspaceId: string | null;
	children: React.ReactNode;
}) {
	const findings = useReviewerFindings(workspaceId);
	const value = useMemo(() => ({ workspaceId, findings }), [findings, workspaceId]);
	return (
		<ReviewerFindingsContext.Provider value={value}>{children}</ReviewerFindingsContext.Provider>
	);
}

export function useReviewerFindingsContext(): ReviewerFindings {
	return useContext(ReviewerFindingsContext);
}
