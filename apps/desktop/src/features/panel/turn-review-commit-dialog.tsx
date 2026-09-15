import { useCallback, useEffect, useRef, useState } from "react";
import type {
	WorkspaceGitChangeEntry,
	WorkspaceGitStatusOutput,
} from "@dcc/contracts";
import { useQueryClient } from "@tanstack/react-query";
import {
	GitCommitHorizontal,
	Loader2,
	Minus,
	Plus,
	RefreshCw,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
	WorkspaceGitFilePreview,
	type WorkspaceGitPreviewSelection,
} from "@/features/inspector/workspace-git-file-preview";
import {
	deriveWorkspaceCommitMessage,
	sanitizeWorkspaceCommitBody,
	sanitizeWorkspaceCommitSubject,
} from "@/features/commit/commit-message";
import {
	isWorkspaceDeliveryBusy,
	setWorkspaceDeliveryBusy,
	useWorkspaceDeliveryBusy,
} from "@/features/commit/workspace-delivery-busy";
import {
	workspaceGitCommit,
	workspaceGitStageFile,
	workspaceGitStatus,
	workspaceGitUnstageFile,
} from "@/lib/workspace-api";
import { TurnReviewStats } from "./turn-review-file-label";

/** Live index review. Opening it never stages files or uses the historical patch. */
export function TurnReviewCommitDialog({
	workspaceRoot,
	onClose,
	onCommitted,
}: {
	workspaceRoot: string;
	onClose: () => void;
	onCommitted: () => Promise<void>;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [snapshot, setSnapshot] = useState<WorkspaceGitStatusOutput | null>(
		null,
	);
	const [selection, setSelection] =
		useState<WorkspaceGitPreviewSelection | null>(null);
	const [message, setMessage] = useState("");
	const [body, setBody] = useState("");
	const [pending, setPending] = useState(true);
	const [error, setError] = useState<string | null>(null);
	const locked = useRef(false);
	const alive = useRef(true);
	const messageEdited = useRef(false);
	const deliveryBusy = useWorkspaceDeliveryBusy(workspaceRoot);
	const busy = pending || deliveryBusy;

	const refresh = useCallback(async () => {
		const status = await workspaceGitStatus({ workspaceRoot });
		if (!alive.current) return;
		setSnapshot(status);
		if (!messageEdited.current)
			setMessage(
				status.staged.length ? deriveWorkspaceCommitMessage(status.staged) : "",
			);
		setSelection((current) => {
			if (current) {
				const updated = status[
					current.group === "staged" ? "staged" : "unstaged"
				].find((file) => file.path === current.path);
				if (updated) return { ...updated, group: current.group };
			}
			const first = status.staged[0] ?? status.unstaged[0];
			return first
				? { ...first, group: status.staged.length ? "staged" : "unstaged" }
				: null;
		});
		queryClient.setQueryData(["workspaceGitStatus", workspaceRoot], status);
		await queryClient.invalidateQueries({
			queryKey: ["workspaceGitFilePreviewContent", workspaceRoot],
		});
	}, [queryClient, workspaceRoot]);

	useEffect(() => {
		alive.current = true;
		void refresh()
			.catch((cause) => {
				if (alive.current) setError(String(cause));
			})
			.finally(() => {
				if (alive.current) setPending(false);
			});
		return () => {
			alive.current = false;
		};
	}, [refresh]);

	async function perform(action: () => Promise<void>) {
		if (locked.current || isWorkspaceDeliveryBusy(workspaceRoot)) return;
		locked.current = true;
		setWorkspaceDeliveryBusy(workspaceRoot, true);
		setPending(true);
		setError(null);
		try {
			await action();
		} catch (cause) {
			if (alive.current)
				setError(cause instanceof Error ? cause.message : String(cause));
		} finally {
			locked.current = false;
			setWorkspaceDeliveryBusy(workspaceRoot, false);
			if (alive.current) setPending(false);
		}
	}

	async function assertBranch() {
		const current = await workspaceGitStatus({ workspaceRoot });
		if (
			!snapshot?.currentBranch ||
			current.currentBranch !== snapshot.currentBranch ||
			current.mergeInProgress ||
			current.conflictCount > 0
		)
			throw new Error(t("turnReview.delivery.changed"));
		return current;
	}

	async function toggleFile(
		file: WorkspaceGitChangeEntry,
		group: "staged" | "unstaged",
	) {
		await perform(async () => {
			await assertBranch();
			await (
				group === "staged" ? workspaceGitUnstageFile : workspaceGitStageFile
			)({ workspaceRoot, relativePath: file.path });
			await refresh();
		});
	}

	const canCommit =
		snapshot &&
		snapshot.staged.length > 0 &&
		snapshot.stagedFingerprint &&
		snapshot.currentBranch &&
		!snapshot.conflictCount &&
		!snapshot.mergeInProgress;
	return (
		<Dialog
			open
			onOpenChange={(open) => {
				if (!open && !busy) onClose();
			}}
		>
			<DialogContent
				className="dcc-review-commit-dialog"
				showCloseButton={!busy}
			>
				<DialogHeader>
					<DialogTitle>{t("turnReview.delivery.commitTitle")}</DialogTitle>
					<DialogDescription>
						{t("turnReview.delivery.commitScope", {
							branch: snapshot?.currentBranch ?? "…",
						})}
					</DialogDescription>
				</DialogHeader>
				{error && (
					<p role="alert" className="text-xs text-destructive">
						{error}
					</p>
				)}
				{snapshot &&
					(!snapshot.currentBranch ||
						snapshot.conflictCount > 0 ||
						snapshot.mergeInProgress) && (
						<p role="status" className="text-xs text-muted-foreground">
							{t("turnReview.delivery.resolveGit")}
						</p>
					)}
				<div className="dcc-review-commit-body">
					<div className="dcc-review-commit-files">
						{(["staged", "unstaged"] as const).map((group) => (
							<section key={group}>
								<h3>
									{t(`turnReview.delivery.${group}`)}{" "}
									<span>{snapshot?.[group].length ?? 0}</span>
								</h3>
								{snapshot?.[group].map((file) => (
									<div key={file.path} className="dcc-review-commit-file">
										<button
											type="button"
											className="dcc-review-commit-file-name"
											title={file.path}
											aria-pressed={
												selection?.path === file.path &&
												selection.group === group
											}
											onClick={() => setSelection({ ...file, group })}
										>
											<span>{file.path}</span>
											<TurnReviewStats {...file} />
										</button>
										<Button
											type="button"
											size="icon-sm"
											variant="ghost"
											disabled={
												busy ||
												!snapshot.currentBranch ||
												snapshot.conflictCount > 0 ||
												snapshot.mergeInProgress
											}
											aria-label={t(
												group === "staged"
													? "turnReview.delivery.unstageFile"
													: "turnReview.delivery.stageFile",
												{ path: file.path },
											)}
											onClick={() => void toggleFile(file, group)}
										>
											{group === "staged" ? (
												<Minus size={14} />
											) : (
												<Plus size={14} />
											)}
										</Button>
									</div>
								))}
							</section>
						))}
						{!pending &&
							snapshot &&
							!snapshot.staged.length &&
							!snapshot.unstaged.length && <p>{t("turnReview.noChanges")}</p>}
					</div>
					<div className="dcc-review-commit-preview">
						{pending && !snapshot ? (
							<Loader2 className="m-auto size-5 animate-spin" />
						) : selection ? (
							<WorkspaceGitFilePreview
								workspaceRoot={workspaceRoot}
								selection={selection}
								forceUnified
							/>
						) : (
							<p className="m-auto p-4 text-xs text-muted-foreground">
								{t("turnReview.selectFile")}
							</p>
						)}
					</div>
				</div>
				<div className="grid gap-2">
					<label className="grid gap-1 text-xs">
						{t("commit.preview.label")}
						<Input
							value={message}
							disabled={busy}
							onChange={(event) => {
								messageEdited.current = true;
								setMessage(event.target.value);
							}}
						/>
					</label>
					<label className="grid gap-1 text-xs">
						{t("commit.preview.bodyLabel")}
						<Textarea
							value={body}
							disabled={busy}
							className="min-h-16"
							onChange={(event) => setBody(event.target.value)}
						/>
					</label>
				</div>
				<div className="flex flex-wrap items-center justify-between gap-2">
					<Button
						type="button"
						variant="ghost"
						size="sm"
						disabled={busy}
						onClick={() => void perform(refresh)}
					>
						<RefreshCw size={14} />
						{t("turnReview.delivery.refresh")}
					</Button>
					<div className="flex gap-2">
						<Button
							type="button"
							variant="outline"
							disabled={busy}
							onClick={onClose}
						>
							{t("commit.preview.cancel")}
						</Button>
						<Button
							type="button"
							disabled={busy || !canCommit || !message.trim()}
							onClick={() =>
								void perform(async () => {
									const current = await assertBranch();
									if (
										!snapshot ||
										current.stagedFingerprint !== snapshot.stagedFingerprint
									)
										throw new Error(t("turnReview.delivery.changed"));
									await workspaceGitCommit({
										workspaceRoot,
										message: sanitizeWorkspaceCommitSubject(message),
										body: sanitizeWorkspaceCommitBody(body),
										stagedFingerprint: snapshot.stagedFingerprint,
									});
									await onCommitted();
									onClose();
								})
							}
						>
							{busy ? (
								<Loader2 size={14} className="animate-spin" />
							) : (
								<GitCommitHorizontal size={14} />
							)}
							{t("turnReview.delivery.confirmCommit", {
								count: snapshot?.staged.length ?? 0,
							})}
						</Button>
					</div>
				</div>
			</DialogContent>
		</Dialog>
	);
}
