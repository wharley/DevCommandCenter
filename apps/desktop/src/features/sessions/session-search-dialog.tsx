import { useDeferredValue, useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Search, History } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { SessionSearchResult } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import {
	CommandDialog,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
} from "@/components/ui/command";
import { sessionSearchQueryOptions } from "./session-search-query";

type SessionSearchDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	selectedWorkspaceId: string | null;
	onSelectResult: (result: SessionSearchResult) => void;
};

function formatUpdatedAt(value: string) {
	const parsed = new Date(value);
	if (Number.isNaN(parsed.getTime())) {
		return value;
	}

	return new Intl.DateTimeFormat(undefined, {
		dateStyle: "medium",
		timeStyle: "short",
	}).format(parsed);
}

function workspaceLabel(result: SessionSearchResult) {
	const workspaceName = result.workspaceName?.trim();
	const workspaceBranch = result.workspaceBranch?.trim();
	if (workspaceName && workspaceBranch) {
		return `${workspaceName} · ${workspaceBranch}`;
	}
	if (workspaceName) {
		return workspaceName;
	}
	if (workspaceBranch) {
		return workspaceBranch;
	}
	return result.workspaceId;
}

export function SessionSearchDialog({
	open,
	onOpenChange,
	selectedWorkspaceId,
	onSelectResult,
}: SessionSearchDialogProps) {
	const { t } = useTranslation("common");
	const [query, setQuery] = useState("");
	const deferredQuery = useDeferredValue(query.trim());
	const resultsQuery = useQuery(
		sessionSearchQueryOptions(open ? deferredQuery : null),
	);
	const results = resultsQuery.data ?? [];

	useEffect(() => {
		if (!open) {
			setQuery("");
		}
	}, [open]);

	const [currentWorkspaceResults, otherWorkspaceResults] = useMemo(() => {
		if (!selectedWorkspaceId) {
			return [[], results] as const;
		}

		return [
			results.filter((result) => result.workspaceId === selectedWorkspaceId),
			results.filter((result) => result.workspaceId !== selectedWorkspaceId),
		] as const;
	}, [results, selectedWorkspaceId]);

	const hasQuery = deferredQuery.length > 0;

	return (
		<CommandDialog
			open={open}
			onOpenChange={onOpenChange}
			title={t("workbench.sessionSearch.title")}
			description={t("workbench.sessionSearch.description")}
		>
			<div className="border-b border-border/60 bg-muted/20 px-4 py-3">
				<p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
					{t("workbench.sessionSearch.title")}
				</p>
				<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
					{hasQuery
						? t("workbench.sessionSearch.searchingDescription")
						: t("workbench.sessionSearch.recentDescription")}
				</p>
			</div>
			<CommandInput
				value={query}
				onValueChange={setQuery}
				placeholder={t("workbench.sessionSearch.placeholder")}
			/>
			<CommandList className="max-h-[min(58vh,32rem)]">
				{resultsQuery.isFetching ? (
					<div className="flex items-center gap-2 px-4 py-3 text-sm text-muted-foreground">
						<Search className="size-4 animate-pulse" strokeWidth={1.8} />
						<span>{t("workbench.sessionSearch.loading")}</span>
					</div>
				) : null}
				<CommandEmpty>{t("workbench.sessionSearch.empty")}</CommandEmpty>
				{currentWorkspaceResults.length > 0 ? (
					<CommandGroup heading={t("workbench.sessionSearch.currentWorkspaceGroup")}>
						{currentWorkspaceResults.map((result) => (
							<CommandItem
								key={result.sessionId}
								value={`${result.threadTitle} ${result.snippet} ${workspaceLabel(result)}`}
								onSelect={() => {
									onSelectResult(result);
									onOpenChange(false);
								}}
								className="items-start gap-3 py-2"
							>
								<History className="mt-0.5 size-4 text-muted-foreground" strokeWidth={1.8} />
								<div className="min-w-0 flex-1">
									<div className="flex items-center gap-2">
										<span className="truncate font-medium">{result.threadTitle}</span>
										{result.archivedAt ? (
											<Badge variant="outline" className="shrink-0 text-[10px]">
												{t("workbench.sessionSearch.archived")}
											</Badge>
										) : null}
									</div>
									<p className="truncate text-xs text-muted-foreground">
										{workspaceLabel(result)}
									</p>
									{result.snippet.trim().length > 0 ? (
										<p className="mt-1 line-clamp-2 text-xs leading-relaxed text-foreground/80">
											{result.snippet}
										</p>
									) : null}
								</div>
								<span className="shrink-0 text-[11px] text-muted-foreground">
									{formatUpdatedAt(result.updatedAt)}
								</span>
							</CommandItem>
						))}
					</CommandGroup>
				) : null}
				{otherWorkspaceResults.length > 0 ? (
					<CommandGroup
						heading={
							selectedWorkspaceId
								? t("workbench.sessionSearch.otherWorkspacesGroup")
								: t("workbench.sessionSearch.allSessionsGroup")
						}
					>
						{otherWorkspaceResults.map((result) => (
							<CommandItem
								key={result.sessionId}
								value={`${result.threadTitle} ${result.snippet} ${workspaceLabel(result)}`}
								onSelect={() => {
									onSelectResult(result);
									onOpenChange(false);
								}}
								className="items-start gap-3 py-2"
							>
								<History className="mt-0.5 size-4 text-muted-foreground" strokeWidth={1.8} />
								<div className="min-w-0 flex-1">
									<div className="flex items-center gap-2">
										<span className="truncate font-medium">{result.threadTitle}</span>
										{result.archivedAt ? (
											<Badge variant="outline" className="shrink-0 text-[10px]">
												{t("workbench.sessionSearch.archived")}
											</Badge>
										) : null}
									</div>
									<p className="truncate text-xs text-muted-foreground">
										{workspaceLabel(result)}
									</p>
									{result.snippet.trim().length > 0 ? (
										<p className="mt-1 line-clamp-2 text-xs leading-relaxed text-foreground/80">
											{result.snippet}
										</p>
									) : null}
								</div>
								<span className="shrink-0 text-[11px] text-muted-foreground">
									{formatUpdatedAt(result.updatedAt)}
								</span>
							</CommandItem>
						))}
					</CommandGroup>
				) : null}
			</CommandList>
		</CommandDialog>
	);
}
