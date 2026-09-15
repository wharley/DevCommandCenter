import type { WorkspaceGitStatusOutput } from "@dcc/contracts";
import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
	ArrowUp,
	GitBranch,
	GitCommitHorizontal,
	GitPullRequest,
	Loader2,
	RefreshCw,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { CreateChangeRequestDialog } from "@/features/commit/CreateChangeRequestDialog";
import { useWorkspaceDelivery } from "@/features/commit/use-workspace-delivery";
import { isWorkspaceDeliveryBusy } from "@/features/commit/workspace-delivery-busy";
import { useWorkspaceGitStatus } from "@/features/inspector/use-workspace-git-status";
import { useWorkspaceForgeContext } from "@/features/inspector/use-workspace-forge-context";
import { useWorkspacePrStatus } from "@/features/inspector/use-workspace-pr-status";
import { useWorkspaceGitBranchDiff } from "@/features/inspector/use-workspace-git-branch-diff";
import { workspaceGitStatus } from "@/lib/workspace-api";
import { TurnReviewCommitDialog } from "./turn-review-commit-dialog";

/** Mounted only while the historical review is open; actions address the live root. */
export function TurnReviewDelivery({
	workspaceRoot,
	onReview,
}: {
	workspaceRoot: string;
	onReview: () => void;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const git = useWorkspaceGitStatus(workspaceRoot, { staleTime: 0 });
	const forge = useWorkspaceForgeContext(workspaceRoot);
	const branch = git.data?.currentBranch ?? null;
	const login = forge.data?.effectiveLogin ?? null;
	const pr = useWorkspacePrStatus(
		forge.isSuccess && branch ? workspaceRoot : null,
		branch,
		login,
		{ staleTime: 0 },
	);
	const branchDiff = useWorkspaceGitBranchDiff(workspaceRoot, { staleTime: 0 });
	const [commitOpen, setCommitOpen] = useState(false);
	const [createBranch, setCreateBranch] = useState<string | null>(null);
	const [pending, setPending] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const locked = useRef(false);
	const alive = useRef(true);
	useEffect(() => {
		alive.current = true;
		return () => {
			alive.current = false;
		};
	}, []);
	const localCount = new Set(
		[...(git.data?.staged ?? []), ...(git.data?.unstaged ?? [])].map(
			(file) => file.path,
		),
	).size;
	const ahead = git.data?.aheadOfRemoteCount ?? 0;
	const behind = git.data?.behindOfRemoteCount ?? 0;
	const requestLabel = forge.data?.provider === "gitlab" ? "MR" : "PR";
	const associatedPr =
		pr.data?.number && pr.data.headBranch === branch ? pr.data : null;
	const base = associatedPr?.baseBranch ?? branchDiff.data?.baseBranch ?? null;
	const remote = forge.data?.remoteName;
	const baseName =
		remote && base?.startsWith(`${remote}/`)
			? base.slice(remote.length + 1)
			: base;
	const ready = git.isSuccess && !git.isFetching;
	const gitBlocked =
		!branch || Boolean(git.data?.conflictCount || git.data?.mergeInProgress);
	const forgeReady =
		forge.isSuccess &&
		forge.data?.status === "ready" &&
		forge.data.remoteState === "ok";
	const delivery = useWorkspaceDelivery({
		workspaceRoot,
		forgeLogin: login,
		baseBranch: base,
		requestLabel,
		stagedCount: git.data?.staged.length ?? 0,
		hasLocalChanges: localCount > 0,
		onReview,
		queryClient,
		t,
	});
	const busy = pending || delivery.busy;
	const canCreate =
		ready &&
		!gitBlocked &&
		forgeReady &&
		pr.isSuccess &&
		!pr.isFetching &&
		branchDiff.isSuccess &&
		!branchDiff.isFetching &&
		!associatedPr &&
		Boolean(
			baseName && branch !== baseName && branchDiff.data.changes.length > 0,
		) &&
		ahead === 0 &&
		behind === 0;

	async function withCurrentBranch(
		action: (current: WorkspaceGitStatusOutput) => Promise<void>,
		expected = branch,
	) {
		if (locked.current || isWorkspaceDeliveryBusy(workspaceRoot)) return;
		locked.current = true;
		setPending(true);
		setError(null);
		try {
			const current = await workspaceGitStatus({ workspaceRoot });
			if (!alive.current) return;
			if (!expected || current.currentBranch !== expected)
				throw new Error(t("turnReview.delivery.changed"));
			queryClient.setQueryData(["workspaceGitStatus", workspaceRoot], current);
			if (isWorkspaceDeliveryBusy(workspaceRoot)) return;
			await action(current);
		} catch (cause) {
			if (alive.current)
				setError(cause instanceof Error ? cause.message : String(cause));
		} finally {
			locked.current = false;
			if (alive.current) setPending(false);
		}
	}

	return (
		<div className="dcc-turn-review-delivery">
			<div className="dcc-turn-review-delivery-context">
				<GitBranch size={14} aria-hidden />
				<span
					className="dcc-turn-review-delivery-branch"
					title={branch ?? undefined}
				>
					{branch ?? t("turnReview.delivery.noBranch")}
				</span>
				<span>{t("turnReview.delivery.liveBranch")}</span>
				{ahead > 0 && (
					<span>{t("turnReview.delivery.ahead", { count: ahead })}</span>
				)}
				{behind > 0 && (
					<span>{t("turnReview.delivery.behind", { count: behind })}</span>
				)}
				{localCount > 0 && (
					<span>{t("turnReview.delivery.local", { count: localCount })}</span>
				)}
			</div>
			<div className="dcc-turn-review-delivery-actions">
				<Button
					type="button"
					size="sm"
					variant="outline"
					disabled={!ready || busy || gitBlocked || localCount === 0}
					onClick={() => setCommitOpen(true)}
				>
					<GitCommitHorizontal size={14} />
					{t("composer.executionDock.actions.commit")}
				</Button>
				<Button
					type="button"
					size="sm"
					variant="outline"
					disabled={
						!ready ||
						busy ||
						gitBlocked ||
						!remote ||
						(ahead === 0 &&
							!(
								branchDiff.data?.changes.length &&
								!associatedPr &&
								branch !== baseName
							)) ||
						behind > 0
					}
					onClick={() =>
						void withCurrentBranch(async (current) => {
							if (
								current.behindOfRemoteCount > 0 ||
								current.conflictCount > 0 ||
								current.mergeInProgress
							)
								throw new Error(t("turnReview.delivery.changed"));
							await delivery.run("push");
						})
					}
				>
					<ArrowUp size={14} />
					{t("commit.modes.push.idle")}
				</Button>
				<Button
					type="button"
					size="sm"
					variant="outline"
					disabled={
						busy ||
						(associatedPr ? !ready || pr.isFetching || pr.isError : !canCreate)
					}
					onClick={() => {
						if (associatedPr)
							void withCurrentBranch(() => delivery.run("open-pr"));
						else if (canCreate) setCreateBranch(branch);
					}}
				>
					<GitPullRequest size={14} />
					{t(
						associatedPr
							? "commit.modes.open-pr.idle"
							: "commit.modes.create-pr.idle",
						{ requestLabel },
					)}
					{associatedPr ? ` #${associatedPr.number}` : ""}
				</Button>
				{busy && (
					<Loader2
						size={14}
						className="animate-spin"
						aria-label={t("turnReview.loading")}
					/>
				)}
			</div>
			<p className="dcc-turn-review-delivery-hint">
				{t("turnReview.delivery.prScope", { requestLabel })}{" "}
				{localCount > 0 || ahead > 0
					? t("turnReview.delivery.notIncluded", { requestLabel })
					: null}
			</p>
			{ahead > 0 && !associatedPr && (
				<p className="dcc-turn-review-delivery-hint">
					{t("turnReview.delivery.pushFirst", { requestLabel })}
				</p>
			)}
			{gitBlocked && ready && (
				<p className="dcc-turn-review-delivery-hint">
					{t("turnReview.delivery.resolveGit")}
				</p>
			)}
			{!forgeReady && !forge.isPending && (
				<p className="dcc-turn-review-delivery-hint">
					{t("turnReview.delivery.connectForge")}
				</p>
			)}
			{branch && branch === baseName && (
				<p className="dcc-turn-review-delivery-hint">
					{t("turnReview.delivery.baseBranch", { requestLabel })}
				</p>
			)}
			{(error ||
				git.isError ||
				forge.isError ||
				pr.isError ||
				branchDiff.isError) && (
				<div role="alert" className="dcc-turn-review-delivery-error">
					<span>{error ?? t("turnReview.delivery.loadFailed")}</span>
					<Button
						type="button"
						size="sm"
						variant="ghost"
						disabled={busy}
						onClick={() => {
							setError(null);
							void delivery.invalidateGitState();
						}}
					>
						<RefreshCw size={13} />
						{t("turnReview.delivery.refresh")}
					</Button>
				</div>
			)}
			{commitOpen && (
				<TurnReviewCommitDialog
					workspaceRoot={workspaceRoot}
					onClose={() => setCommitOpen(false)}
					onCommitted={async () => {
						toast.success(t("composer.executionDock.actions.committed"));
						await delivery.invalidateGitState();
					}}
				/>
			)}
			{createBranch && (
				<CreateChangeRequestDialog
					open
					onOpenChange={(open) => {
						if (!open) setCreateBranch(null);
					}}
					requestLabel={requestLabel}
					headBranch={createBranch}
					baseBranch={baseName}
					defaultTitle={createBranch
						.replace(/^[^/]+\//, "")
						.replace(/[-_]/g, " ")}
					localFiles={localCount}
					localAdditions={0}
					localDeletions={0}
					allowIncludeLocalChanges={false}
					scopeDescription={t("turnReview.delivery.createScope", {
						requestLabel,
					})}
					loading={busy}
					submitDisabled={!canCreate || createBranch !== branch}
					errorMessage={
						error ??
						(createBranch !== branch || gitBlocked || behind > 0
							? t("turnReview.delivery.changed")
							: ahead > 0
								? t("turnReview.delivery.pushFirst", { requestLabel })
								: git.isError ||
									  forge.isError ||
									  pr.isError ||
									  branchDiff.isError
									? t("turnReview.delivery.loadFailed")
									: null)
					}
					onSubmit={async (input) => {
						await withCurrentBranch(async (current) => {
							if (
								current.aheadOfRemoteCount > 0 ||
								current.behindOfRemoteCount > 0 ||
								current.conflictCount > 0 ||
								current.mergeInProgress
							)
								throw new Error(t("turnReview.delivery.changed"));
							await delivery.createRequest({
								workspaceRoot,
								forgeLogin: login,
								title: input.title,
								body: input.body || null,
								draft: input.draft,
								includeLocalChanges: false,
							});
							setCreateBranch(null);
						}, createBranch);
					}}
				/>
			)}
		</div>
	);
}
