import { createContext, useContext } from "react";
import type { ReviewFinding } from "./review-findings";
import { useReviewerFindings } from "./use-reviewer-findings";

const ReviewerFindingsContext = createContext<ReviewFinding[]>([]);

/** Makes the task's current reviewer findings available to every diff below. */
export function ReviewerFindingsProvider({
	workspaceId,
	children,
}: {
	workspaceId: string | null;
	children: React.ReactNode;
}) {
	return (
		<ReviewerFindingsContext.Provider value={useReviewerFindings(workspaceId)}>
			{children}
		</ReviewerFindingsContext.Provider>
	);
}

export function useReviewerFindingsContext(): ReviewFinding[] {
	return useContext(ReviewerFindingsContext);
}
