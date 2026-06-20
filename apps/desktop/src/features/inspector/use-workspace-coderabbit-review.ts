import { useCallback, useEffect, useState } from "react";
import type { WorkspaceCodeRabbitReviewOutput } from "@dcc/contracts";

function storageKey(workspaceRoot: string): string {
	return `dcc.workspace.coderabbit.review.${encodeURIComponent(workspaceRoot)}`;
}

export function loadStoredCodeRabbitReview(
	workspaceRoot: string,
): WorkspaceCodeRabbitReviewOutput | null {
	if (typeof window === "undefined") {
		return null;
	}
	try {
		const raw = window.localStorage.getItem(storageKey(workspaceRoot));
		if (!raw) {
			return null;
		}
		return JSON.parse(raw) as WorkspaceCodeRabbitReviewOutput;
	} catch {
		return null;
	}
}

export function saveStoredCodeRabbitReview(
	workspaceRoot: string,
	review: WorkspaceCodeRabbitReviewOutput,
) {
	if (typeof window === "undefined") {
		return;
	}
	window.localStorage.setItem(storageKey(workspaceRoot), JSON.stringify(review));
}

export function clearStoredCodeRabbitReview(workspaceRoot: string) {
	if (typeof window === "undefined") {
		return;
	}
	window.localStorage.removeItem(storageKey(workspaceRoot));
}

export function useStoredCodeRabbitReview(workspaceRoot: string | null) {
	const root = workspaceRoot?.trim() ?? "";
	const [review, setReview] = useState<WorkspaceCodeRabbitReviewOutput | null>(() =>
		root ? loadStoredCodeRabbitReview(root) : null,
	);

	useEffect(() => {
		setReview(root ? loadStoredCodeRabbitReview(root) : null);
	}, [root]);

	const saveReview = useCallback(
		(next: WorkspaceCodeRabbitReviewOutput) => {
			if (!root) {
				return;
			}
			saveStoredCodeRabbitReview(root, next);
			setReview(next);
		},
		[root],
	);

	const clearReview = useCallback(() => {
		if (!root) {
			return;
		}
		clearStoredCodeRabbitReview(root);
		setReview(null);
	}, [root]);

	return { review, saveReview, clearReview };
}
