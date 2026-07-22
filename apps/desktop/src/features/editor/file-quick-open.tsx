import { useQuery } from "@tanstack/react-query";
import { FileCode } from "lucide-react";
import { useDeferredValue, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	CommandDialog,
	CommandEmpty,
	CommandInput,
	CommandItem,
	CommandList,
} from "@/components/ui/command";
import { listGitTrackedFiles } from "@/lib/workspace-api";

const MAX_RESULTS = 60;

type FileQuickOpenProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	workspaceRoot: string | null;
	/** Open the chosen file in the read-only file surface. */
	onSelectFile: (input: { path: string; name: string }) => void;
};

function basename(path: string): string {
	const slash = path.lastIndexOf("/");
	return slash === -1 ? path : path.slice(slash + 1);
}

function dirname(path: string): string {
	const slash = path.lastIndexOf("/");
	return slash === -1 ? "" : path.slice(0, slash);
}

/**
 * Case-insensitive subsequence score. Returns -1 when `needle` is not a
 * subsequence of `haystack`; otherwise higher is better. Consecutive runs and
 * matches near the basename are rewarded so file names beat deep path noise.
 */
function fuzzyScore(haystack: string, needle: string, basenameStart: number): number {
	if (needle.length === 0) return 0;
	const hay = haystack.toLowerCase();
	const need = needle.toLowerCase();

	let score = 0;
	let hayIndex = 0;
	let prevMatch = -2;
	for (let i = 0; i < need.length; i += 1) {
		const ch = need[i];
		const found = hay.indexOf(ch, hayIndex);
		if (found === -1) return -1;
		score += 1;
		if (found === prevMatch + 1) score += 4; // consecutive run
		if (found >= basenameStart) score += 3; // inside the file name
		prevMatch = found;
		hayIndex = found + 1;
	}
	return score;
}

export function rankQuickOpenFiles(
	paths: readonly string[],
	query: string,
): string[] {
	if (query.length === 0) {
		return paths.slice(0, MAX_RESULTS);
	}

	const normalizedQuery = query.toLowerCase();
	const scored: { path: string; score: number }[] = [];
	for (const path of paths) {
		const normalizedPath = path.toLowerCase();
		const name = basename(path).toLowerCase();
		let score: number;

		// A literal filename/path match must always be visible first. This keeps
		// an exact result from landing below fuzzy matches and requiring a scroll.
		if (normalizedPath === normalizedQuery) {
			score = 10_000;
		} else if (name === normalizedQuery) {
			score = 9_000;
		} else if (normalizedPath.includes(normalizedQuery)) {
			score = 8_000;
		} else {
			score = fuzzyScore(path, query, dirname(path).length + 1);
		}

		if (score >= 0) scored.push({ path, score });
	}

	scored.sort((a, b) => b.score - a.score || a.path.length - b.path.length);
	return scored.slice(0, MAX_RESULTS).map((item) => item.path);
}

export function FileQuickOpen({
	open,
	onOpenChange,
	workspaceRoot,
	onSelectFile,
}: FileQuickOpenProps) {
	const { t } = useTranslation("common");
	const [query, setQuery] = useState("");
	const deferredQuery = useDeferredValue(query.trim());

	useEffect(() => {
		if (!open) {
			setQuery("");
		}
	}, [open]);

	const filesQuery = useQuery({
		queryKey: ["gitTrackedFiles", workspaceRoot ?? ""],
		queryFn: async () => {
			if (!workspaceRoot) return [] as string[];
			const result = await listGitTrackedFiles({ workspaceRoot });
			return result.paths;
		},
		enabled: open && Boolean(workspaceRoot),
		staleTime: 30_000,
		refetchOnWindowFocus: false,
	});

	const results = useMemo(() => {
		const paths = filesQuery.data ?? [];
		return rankQuickOpenFiles(paths, deferredQuery);
	}, [filesQuery.data, deferredQuery]);

	return (
		<CommandDialog
			open={open}
			onOpenChange={onOpenChange}
			title={t("quickOpen.title")}
			description={t("quickOpen.description")}
			// Filtering is done here (bounded to the top matches) so the list never
			// renders thousands of rows; cmdk's own filter would keep them all mounted.
			shouldFilter={false}
		>
			<CommandInput
				value={query}
				onValueChange={setQuery}
				placeholder={t("quickOpen.placeholder")}
			/>
			<CommandList className="max-h-[min(58vh,32rem)]">
				{filesQuery.isFetching && results.length === 0 ? (
					<div className="px-4 py-3 text-sm text-muted-foreground">
						{t("quickOpen.loading")}
					</div>
				) : (
					<CommandEmpty>{t("quickOpen.empty")}</CommandEmpty>
				)}
				{results.map((path) => {
					const name = basename(path);
					const dir = dirname(path);
					return (
						<CommandItem
							key={path}
							value={path}
							onSelect={() => {
								onSelectFile({ path, name });
								onOpenChange(false);
							}}
							className="items-center gap-2.5 py-1.5"
						>
							<FileCode
								className="size-4 shrink-0 text-muted-foreground"
								strokeWidth={1.8}
							/>
							<span className="shrink-0 truncate font-medium">{name}</span>
							{dir ? (
								<span
									className="min-w-0 flex-1 truncate text-right font-mono text-[11px] text-muted-foreground"
									dir="rtl"
								>
									{dir}
								</span>
							) : null}
						</CommandItem>
					);
				})}
			</CommandList>
		</CommandDialog>
	);
}
