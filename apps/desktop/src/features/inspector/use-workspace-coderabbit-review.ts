import { useCallback } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import type {
	WorkspaceCodeRabbitReviewOutput,
	WorkspaceCodeRabbitStoredReviewOutput,
} from "@dcc/contracts";
import {
	workspaceCodeRabbitReviewClear,
	workspaceCodeRabbitReviewHistory,
	workspaceCodeRabbitReviewLoad,
	workspaceCodeRabbitReviewSave,
} from "@/lib/workspace-api";

const WORKSPACE_CODERABBIT_REVIEW_QUERY_KEY = "workspaceCodeRabbitStoredReview";
const WORKSPACE_CODERABBIT_HISTORY_QUERY_KEY = "workspaceCodeRabbitReviewHistory";

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

function reviewQueryKey(workspaceRoot: string) {
	return [WORKSPACE_CODERABBIT_REVIEW_QUERY_KEY, workspaceRoot] as const;
}

function historyQueryKey(workspaceRoot: string) {
	return [WORKSPACE_CODERABBIT_HISTORY_QUERY_KEY, workspaceRoot] as const;
}

async function loadPersistedCodeRabbitReview(
	workspaceRoot: string,
): Promise<WorkspaceCodeRabbitStoredReviewOutput> {
	const legacyReview = loadStoredCodeRabbitReview(workspaceRoot);
	try {
		const loaded = await workspaceCodeRabbitReviewLoad({ workspaceRoot });
		if (loaded.review || !legacyReview) {
			if (loaded.review) {
				clearStoredCodeRabbitReview(workspaceRoot);
			}
			return loaded;
		}
		const migrated = await workspaceCodeRabbitReviewSave({
			workspaceRoot,
			review: legacyReview,
		});
		clearStoredCodeRabbitReview(workspaceRoot);
		return migrated;
	} catch {
		return {
			workspaceRoot,
			review: legacyReview,
			updatedAt: null,
		};
	}
}

export function useStoredCodeRabbitReview(workspaceRoot: string | null) {
	const root = workspaceRoot?.trim() ?? "";
	const queryClient = useQueryClient();
	const query = useQuery({
		queryKey: reviewQueryKey(root),
		queryFn: () => loadPersistedCodeRabbitReview(root),
		enabled: Boolean(root),
		staleTime: 30_000,
		initialData: root
			? {
					workspaceRoot: root,
					review: loadStoredCodeRabbitReview(root),
					updatedAt: null,
				}
			: undefined,
	});
	const historyQuery = useQuery({
		queryKey: historyQueryKey(root),
		queryFn: () => workspaceCodeRabbitReviewHistory({ workspaceRoot: root, limit: 12 }),
		enabled: Boolean(root),
		staleTime: 30_000,
	});

	const saveReview = useCallback(
		(next: WorkspaceCodeRabbitReviewOutput) => {
			if (!root) {
				return;
			}
			queryClient.setQueryData<WorkspaceCodeRabbitStoredReviewOutput>(
				reviewQueryKey(root),
				{
					workspaceRoot: root,
					review: next,
					updatedAt: new Date().toISOString(),
				},
			);
			void workspaceCodeRabbitReviewSave({ workspaceRoot: root, review: next })
				.then((saved) => {
					clearStoredCodeRabbitReview(root);
					queryClient.setQueryData(reviewQueryKey(root), saved);
					void queryClient.invalidateQueries({ queryKey: historyQueryKey(root) });
				})
				.catch(() => saveStoredCodeRabbitReview(root, next));
		},
		[queryClient, root],
	);

	const clearReview = useCallback(() => {
		if (!root) {
			return;
		}
		clearStoredCodeRabbitReview(root);
		queryClient.setQueryData<WorkspaceCodeRabbitStoredReviewOutput>(
			reviewQueryKey(root),
			{
				workspaceRoot: root,
				review: null,
				updatedAt: null,
			},
		);
		void workspaceCodeRabbitReviewClear({ workspaceRoot: root });
	}, [queryClient, root]);

	const useReviewFromHistory = useCallback(
		(next: WorkspaceCodeRabbitReviewOutput) => {
			if (!root) {
				return;
			}
			queryClient.setQueryData<WorkspaceCodeRabbitStoredReviewOutput>(
				reviewQueryKey(root),
				{
					workspaceRoot: root,
					review: next,
					updatedAt: new Date().toISOString(),
				},
			);
		},
		[queryClient, root],
	);

	return {
		review: query.data?.review ?? null,
		saveReview,
		clearReview,
		history: historyQuery.data?.entries ?? [],
		historyLoading: historyQuery.isPending,
		useReviewFromHistory,
	};
}
