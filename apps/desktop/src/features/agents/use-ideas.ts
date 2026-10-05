import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { dccQueryKeys } from "@/lib/query-client";
import { type Idea, listIdeas } from "@/lib/research-api";

const NO_IDEAS: Idea[] = [];

/**
 * The ideas in progress and their folders. An idea's repository and task stay
 * out of the project lists; only the researcher's page leads to them.
 */
export function useIdeas(): { ideas: Idea[]; ideaRootPaths: ReadonlySet<string> } {
	const query = useQuery({
		queryKey: dccQueryKeys.ideas,
		queryFn: listIdeas,
		staleTime: 30_000,
		retry: false,
	});
	const ideas = query.data ?? NO_IDEAS;
	const ideaRootPaths = useMemo(() => new Set(ideas.map((idea) => idea.rootPath)), [ideas]);
	return { ideas, ideaRootPaths };
}
