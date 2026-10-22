import type { WorkspaceReviewerCandidate } from "@dcc/contracts";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { Loader2, UserPlus } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Command,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
} from "@/components/ui/command";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import { workspaceReviewerCandidates } from "@/lib/workspace-api";

export const WORKSPACE_REVIEWER_CANDIDATES_QUERY_KEY =
	"workspaceReviewerCandidates";
const SEARCH_DEBOUNCE_MS = 250;

/** Candidates not already on the review, split into suggested and the rest. */
export function groupReviewerCandidates(
	candidates: readonly WorkspaceReviewerCandidate[],
	excludedLogins: readonly string[],
) {
	const excluded = new Set(excludedLogins.map((login) => login.toLowerCase()));
	const available = candidates.filter(
		(candidate) => !excluded.has(candidate.login.toLowerCase()),
	);
	return {
		suggested: available.filter((candidate) => candidate.suggested),
		others: available.filter((candidate) => !candidate.suggested),
	};
}

function CandidateAvatar({ candidate }: { candidate: WorkspaceReviewerCandidate }) {
	return (
		<span className="flex size-5 shrink-0 items-center justify-center overflow-hidden rounded-full bg-muted text-[8px] font-semibold text-muted-foreground ring-1 ring-border/70">
			{candidate.avatarUrl ? (
				<img src={candidate.avatarUrl} alt="" className="size-full object-cover" />
			) : (
				candidate.login.slice(0, 2).toUpperCase()
			)}
		</span>
	);
}

export function RequestReviewPicker({
	workspaceRoot,
	number,
	forgeLogin,
	excludedLogins,
	busy,
	onPick,
}: {
	workspaceRoot: string;
	number: number;
	forgeLogin: string | null;
	excludedLogins: readonly string[];
	busy: boolean;
	onPick: (login: string) => void;
}) {
	const { t } = useTranslation("common");
	const [open, setOpen] = useState(false);
	const [search, setSearch] = useState("");
	const [debouncedSearch, setDebouncedSearch] = useState("");

	useEffect(() => {
		const timer = setTimeout(
			() => setDebouncedSearch(search.trim()),
			SEARCH_DEBOUNCE_MS,
		);
		return () => clearTimeout(timer);
	}, [search]);

	const candidates = useQuery({
		queryKey: [
			WORKSPACE_REVIEWER_CANDIDATES_QUERY_KEY,
			workspaceRoot,
			number,
			forgeLogin ?? "",
			debouncedSearch,
		],
		queryFn: () =>
			workspaceReviewerCandidates({
				workspaceRoot,
				number,
				query: debouncedSearch || null,
				forgeLogin,
			}),
		enabled: open,
		staleTime: 60_000,
		placeholderData: keepPreviousData,
	});
	const { suggested, others } = groupReviewerCandidates(
		candidates.data?.candidates ?? [],
		excludedLogins,
	);

	const pick = (login: string) => {
		setOpen(false);
		setSearch("");
		onPick(login);
	};

	const renderItem = (candidate: WorkspaceReviewerCandidate) => (
		<CommandItem
			key={candidate.login}
			value={candidate.login}
			onSelect={() => pick(candidate.login)}
			className="gap-2 text-[11px]"
		>
			<CandidateAvatar candidate={candidate} />
			<span className="min-w-0 truncate font-medium">@{candidate.login}</span>
			{candidate.name ? (
				<span className="min-w-0 flex-1 truncate text-[10px] text-muted-foreground">
					{candidate.name}
				</span>
			) : null}
		</CommandItem>
	);

	return (
		<Popover
			open={open}
			onOpenChange={(next) => {
				setOpen(next);
				if (!next) setSearch("");
			}}
		>
			<PopoverTrigger asChild>
				<Button
					type="button"
					variant="ghost"
					size="xs"
					className="h-5 gap-1 px-1.5 text-[9.5px] text-muted-foreground hover:text-foreground"
					disabled={busy}
					data-request-review-trigger
				>
					{busy ? (
						<Loader2 className="size-3 animate-spin" />
					) : (
						<UserPlus className="size-3" />
					)}
					{t("inspector.reviewState.requestReview")}
				</Button>
			</PopoverTrigger>
			<PopoverContent align="end" sideOffset={6} className="w-64 overflow-hidden p-0">
				<Command shouldFilter={false} className="bg-transparent">
					<CommandInput
						value={search}
						onValueChange={setSearch}
						placeholder={t("inspector.reviewState.requestReviewSearch")}
					/>
					<CommandList className="max-h-64">
						{candidates.isPending ? (
							<div className="flex items-center gap-2 px-3 py-3 text-[11px] text-muted-foreground">
								<Loader2 className="size-3.5 animate-spin" />
								{t("inspector.reviewState.requestReviewLoading")}
							</div>
						) : candidates.isError ? (
							<div className="px-3 py-3 text-[11px] text-amber-700 dark:text-amber-300">
								{t("inspector.reviewState.requestReviewLoadFailed")}
							</div>
						) : (
							<>
								<CommandEmpty className="py-4 text-center text-[11px] text-muted-foreground">
									{t("inspector.reviewState.requestReviewEmpty")}
								</CommandEmpty>
								{suggested.length > 0 ? (
									<CommandGroup
										heading={t("inspector.reviewState.requestReviewSuggested")}
									>
										{suggested.map(renderItem)}
									</CommandGroup>
								) : null}
								{others.length > 0 ? (
									<CommandGroup
										heading={t("inspector.reviewState.requestReviewPeople")}
									>
										{others.map(renderItem)}
									</CommandGroup>
								) : null}
							</>
						)}
					</CommandList>
				</Command>
			</PopoverContent>
		</Popover>
	);
}
