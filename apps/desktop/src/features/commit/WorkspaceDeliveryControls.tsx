import { ChevronDown, FileDiff, LoaderCircle } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
	resolveExecutionDockActions,
	type ExecutionDockAction,
	type ExecutionDockRunMode,
} from "@/features/composer/ExecutionDock.actions";
import {
	resolveExecutionDockStatus,
	type ExecutionDockChangeSummary,
	type ExecutionDockGitState,
} from "@/features/composer/ExecutionDock.logic";
import { commitTranslationKey, type CommitMode } from "./WorkspaceCommitButton.logic";

type WorkspaceDeliveryControlsProps = {
	changeSummary: ExecutionDockChangeSummary | null;
	gitStatusState?: ExecutionDockGitState;
	commitMode?: CommitMode | null;
	forgeRequestLabel?: "PR" | "MR";
	deliveryBusy?: boolean;
	multiProjectCount?: number;
	onReviewChanges?: () => void;
	onRunDeliveryAction?: (mode: ExecutionDockRunMode) => Promise<void> | void;
	onCreateChangeRequest?: (draft: boolean) => void;
	onOpenMultiProjectDelivery?: () => void;
};

export function WorkspaceDeliveryControls({
	changeSummary,
	gitStatusState = "ready",
	commitMode = null,
	forgeRequestLabel = "PR",
	deliveryBusy = false,
	multiProjectCount = 1,
	onReviewChanges,
	onRunDeliveryAction,
	onCreateChangeRequest,
	onOpenMultiProjectDelivery,
}: WorkspaceDeliveryControlsProps) {
	const { t } = useTranslation("common");
	const multiProject = multiProjectCount > 1;
	const gitStatus = resolveExecutionDockStatus(changeSummary, gitStatusState);
	const actionLoading = deliveryBusy || gitStatusState !== "ready";
	const deliveryActions = useMemo(
		() =>
			resolveExecutionDockActions({
				mode: commitMode,
				loading: actionLoading,
				multiProject,
				hasLocalChanges: (changeSummary?.files ?? 0) > 0,
				hasBranchChanges: (changeSummary?.branchFiles ?? 0) > 0,
				hasAheadCommits: (changeSummary?.aheadOfRemoteCount ?? 0) > 0,
				hasChangeRequest: Boolean(changeSummary?.pullRequestState?.trim()),
				hasOpenRequest:
					changeSummary?.pullRequestState?.trim().toLowerCase() === "open",
			}),
		[actionLoading, changeSummary, commitMode, multiProject],
	);
	const primaryAction = deliveryActions[0];

	function actionLabel(action: ExecutionDockAction) {
		if (multiProject) {
			return t("workspaceScope.delivery.action", { count: multiProjectCount });
		}
		if (action.id === "sync-base" || action.mode === "sync-base") {
			return t("composer.executionDock.actions.syncBase");
		}
		if (action.id === "create-draft-pr") {
			return t("composer.executionDock.actions.createDraft", {
				requestLabel: forgeRequestLabel,
			});
		}
		if (action.mode === "commit") {
			return t("composer.executionDock.actions.commit");
		}
		if (action.mode) {
			return t(commitTranslationKey(action.mode, "idle"), {
				requestLabel: forgeRequestLabel,
			});
		}
		return t("composer.executionDock.actions.git");
	}

	function runAction(action: ExecutionDockAction) {
		if (action.disabled) return;
		if (action.kind === "create-request") {
			onCreateChangeRequest?.(action.id === "create-draft-pr");
			return;
		}
		if (action.mode) {
			void onRunDeliveryAction?.(action.mode);
			return;
		}
		if (action.id === "primary" && multiProject) {
			onOpenMultiProjectDelivery?.();
		}
	}

	const localAdditions =
		gitStatus.kind === "local"
			? gitStatus.additions
			: gitStatus.kind === "local-and-branch"
				? gitStatus.localAdditions
				: gitStatus.kind === "branch"
					? gitStatus.additions
					: 0;
	const localDeletions =
		gitStatus.kind === "local"
			? gitStatus.deletions
			: gitStatus.kind === "local-and-branch"
				? gitStatus.localDeletions
				: gitStatus.kind === "branch"
					? gitStatus.deletions
					: 0;

	return (
		<div className="flex items-center gap-1.5">
			<Button
				type="button"
				variant="ghost"
				size="sm"
				disabled={!onReviewChanges}
				onClick={onReviewChanges}
				className="h-8 gap-1.5 px-2 text-[11px] text-muted-foreground hover:text-foreground"
				aria-label={t("composer.executionDock.reviewChanges")}
			>
				<FileDiff className="size-3.5" strokeWidth={1.8} />
				{gitStatus.kind === "loading" ? (
					<LoaderCircle className="size-3 animate-spin" />
				) : localAdditions > 0 || localDeletions > 0 ? (
					<span className="tabular-nums">
						{localAdditions > 0 ? (
							<span className="text-emerald-500">+{localAdditions}</span>
						) : null}
						{localDeletions > 0 ? (
							<span className="ml-1 text-destructive">−{localDeletions}</span>
						) : null}
					</span>
				) : (
					<span className="text-muted-foreground/65">0</span>
				)}
			</Button>

			<DropdownMenu>
				<DropdownMenuTrigger asChild>
					<Button
						type="button"
						variant="outline"
						size="sm"
						disabled={actionLoading}
						className="h-8 max-w-44 gap-1.5 px-2.5 text-[11px]"
					>
						{deliveryBusy ? <LoaderCircle className="size-3 animate-spin" /> : null}
						<span className="truncate">{actionLabel(primaryAction)}</span>
						<ChevronDown className="size-3" />
					</Button>
				</DropdownMenuTrigger>
				<DropdownMenuContent align="end" className="w-64">
					<DropdownMenuLabel>{t("composer.executionDock.actions.title")}</DropdownMenuLabel>
					{deliveryActions.map((action, index) => (
						<DropdownMenuItem
							key={`${action.id}-${index}`}
							disabled={action.disabled}
							onSelect={() => runAction(action)}
							className={action.primary ? "font-medium" : undefined}
						>
							{actionLabel(action)}
						</DropdownMenuItem>
					))}
				</DropdownMenuContent>
			</DropdownMenu>
		</div>
	);
}
