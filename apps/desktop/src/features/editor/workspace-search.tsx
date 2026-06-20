import { useQuery } from "@tanstack/react-query";
import type { SearchWorkspaceMatch } from "@dcc/contracts";
import { Search } from "lucide-react";
import { Fragment, useDeferredValue, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	CommandDialog,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
} from "@/components/ui/command";
import { searchWorkspace } from "@/lib/workspace-api";

const MIN_QUERY_LENGTH = 2;

type WorkspaceSearchProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	workspaceRoot: string | null;
	/** Open the file at the matched line in the read-only file surface. */
	onSelectMatch: (input: { path: string; line: number }) => void;
};

function basename(path: string): string {
	const slash = path.lastIndexOf("/");
	return slash === -1 ? path : path.slice(slash + 1);
}

/** Splits `text` around the first case-insensitive occurrence of `query`. */
function highlight(text: string, query: string) {
	if (!query) return [text] as const;
	const at = text.toLowerCase().indexOf(query.toLowerCase());
	if (at === -1) return [text] as const;
	return [
		text.slice(0, at),
		text.slice(at, at + query.length),
		text.slice(at + query.length),
	] as const;
}

export function WorkspaceSearch({
	open,
	onOpenChange,
	workspaceRoot,
	onSelectMatch,
}: WorkspaceSearchProps) {
	const { t } = useTranslation("common");
	const [query, setQuery] = useState("");
	const deferredQuery = useDeferredValue(query.trim());

	useEffect(() => {
		if (!open) {
			setQuery("");
		}
	}, [open]);

	const enabled =
		open && Boolean(workspaceRoot) && deferredQuery.length >= MIN_QUERY_LENGTH;

	const searchQuery = useQuery({
		queryKey: ["workspaceSearch", workspaceRoot ?? "", deferredQuery],
		queryFn: () =>
			searchWorkspace({
				workspaceRoot: workspaceRoot ?? "",
				query: deferredQuery,
			}),
		enabled,
		staleTime: 15_000,
		refetchOnWindowFocus: false,
	});

	// Group matches by file so results read as "file → lines", like an IDE search.
	const groups = useMemo(() => {
		const byPath = new Map<string, SearchWorkspaceMatch[]>();
		for (const match of searchQuery.data?.matches ?? []) {
			const list = byPath.get(match.path);
			if (list) list.push(match);
			else byPath.set(match.path, [match]);
		}
		return [...byPath.entries()];
	}, [searchQuery.data]);

	const hasQuery = deferredQuery.length >= MIN_QUERY_LENGTH;

	return (
		<CommandDialog
			open={open}
			onOpenChange={onOpenChange}
			title={t("workspaceSearch.title")}
			description={t("workspaceSearch.description")}
			shouldFilter={false}
		>
			<CommandInput
				value={query}
				onValueChange={setQuery}
				placeholder={t("workspaceSearch.placeholder")}
			/>
			<CommandList className="max-h-[min(58vh,32rem)]">
				{enabled && searchQuery.isFetching && groups.length === 0 ? (
					<div className="flex items-center gap-2 px-4 py-3 text-sm text-muted-foreground">
						<Search className="size-4 animate-pulse" strokeWidth={1.8} />
						<span>{t("workspaceSearch.searching")}</span>
					</div>
				) : !hasQuery ? (
					<div className="px-4 py-3 text-sm text-muted-foreground">
						{t("workspaceSearch.hint")}
					</div>
				) : (
					<CommandEmpty>{t("workspaceSearch.empty")}</CommandEmpty>
				)}
				{groups.map(([path, matches]) => (
					<CommandGroup
						key={path}
						heading={
							<span className="font-mono text-[11px]">
								<span className="text-foreground/80">{basename(path)}</span>
								<span className="text-muted-foreground"> · {path}</span>
							</span>
						}
					>
						{matches.map((match) => {
							const [before, hit, after] = highlight(
								match.text.trim(),
								deferredQuery,
							);
							return (
								<CommandItem
									key={`${path}:${match.line}`}
									value={`${path}:${match.line}`}
									onSelect={() => {
										onSelectMatch({ path, line: match.line });
										onOpenChange(false);
									}}
									className="items-baseline gap-2.5 py-1.5"
								>
									<span className="w-10 shrink-0 text-right font-mono text-[11px] tabular-nums text-muted-foreground">
										{match.line}
									</span>
									<span className="min-w-0 flex-1 truncate font-mono text-[12px] text-muted-foreground">
										{before}
										{hit ? (
											<Fragment>
												<mark className="rounded-sm bg-primary/20 text-foreground">
													{hit}
												</mark>
											</Fragment>
										) : null}
										{after}
									</span>
								</CommandItem>
							);
						})}
					</CommandGroup>
				))}
				{searchQuery.data?.truncated ? (
					<div className="px-4 py-2 text-[11px] text-muted-foreground">
						{t("workspaceSearch.truncated")}
					</div>
				) : null}
			</CommandList>
		</CommandDialog>
	);
}
