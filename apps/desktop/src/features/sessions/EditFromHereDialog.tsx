import { AlertTriangle, GitFork, Loader2, RotateCcw, Undo2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import {
	editFromHereActions,
	editFromHereContinuation,
	editFromHereFiles,
	type EditFromHereOutcome,
	type EditFromHerePlan,
} from "./edit-from-here.logic";

const VISIBLE_FILES = 8;

/**
 * Confirms "edit from here". Restoring files is its own button: closing the
 * dialog, cancelling or going back without restoring never touches a file.
 */
export function EditFromHereDialog({
	plan,
	working,
	stopped,
	onCancel,
	onConfirm,
}: {
	plan: EditFromHerePlan | null;
	working: boolean;
	/** Restoring stopped; shown instead of the choices. */
	stopped: Extract<EditFromHereOutcome, { kind: "files_stopped" }> | null;
	onCancel: () => void;
	onConfirm: (restoreFiles: boolean) => void;
}) {
	const { t } = useTranslation("common");
	const open = plan !== null;
	const continuation = plan ? editFromHereContinuation(plan) : null;
	const files = plan ? editFromHereFiles(plan) : null;
	const actions = plan ? editFromHereActions(plan) : [];
	const newThread = continuation?.kind === "new_thread";
	const previews = plan?.files.status === "restorable" ? plan.files.files : [];

	return (
		<Dialog
			open={open}
			onOpenChange={(next) => {
				if (!next && !working) onCancel();
			}}
		>
			<DialogContent
				className="flex max-h-[86vh] max-w-xl flex-col overflow-hidden"
				showCloseButton={!working}
				data-testid="edit-from-here-dialog"
			>
				<DialogHeader>
					<DialogTitle>{t("conversation.editFromHere.title")}</DialogTitle>
					<DialogDescription>
						{plan
							? t("conversation.editFromHere.summary", {
									count: plan.removedTurnIds.length,
								})
							: null}
					</DialogDescription>
				</DialogHeader>
				{plan && continuation && files ? (
					<div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1 text-xs">
						<section className="space-y-1" data-testid="edit-from-here-context">
							<p className="font-medium text-foreground">
								{t("conversation.editFromHere.contextLabel")}
							</p>
							<p className="text-muted-foreground">
								{continuation.kind === "native"
									? t("conversation.editFromHere.contextNative")
									: continuation.kind === "fresh"
										? t("conversation.editFromHere.contextFresh")
										: t("conversation.editFromHere.contextNewThread", {
												reason: t(continuation.reasonKey),
											})}
							</p>
						</section>
						<section className="space-y-1.5" data-testid="edit-from-here-files">
							<p className="font-medium text-foreground">
								{t("conversation.editFromHere.filesLabel")}
							</p>
							{files.kind === "none" ? (
								<p className="text-muted-foreground">{t("conversation.editFromHere.filesNone")}</p>
							) : files.kind === "blocked" ? (
								<p className="flex gap-2 rounded-md border border-amber-500/40 bg-amber-500/5 p-2.5 text-amber-800 dark:text-amber-300">
									<AlertTriangle className="mt-0.5 size-3.5 shrink-0" aria-hidden />
									{t("conversation.editFromHere.filesBlocked", {
										reason: t(files.reasonKey),
									})}
								</p>
							) : (
								<>
									<p className="text-muted-foreground">
										{t("conversation.editFromHere.filesRestorable", {
											count: files.fileCount,
										})}
									</p>
									<ul className="space-y-0.5 font-mono text-[11px] text-muted-foreground">
										{previews.slice(0, VISIBLE_FILES).map((file) => (
											<li key={`${file.turnId}:${file.displayPath}`} className="truncate">
												{file.displayPath}
											</li>
										))}
										{previews.length > VISIBLE_FILES ? (
											<li>
												{t("conversation.editFromHere.filesMore", {
													count: previews.length - VISIBLE_FILES,
												})}
											</li>
										) : null}
									</ul>
								</>
							)}
						</section>
						{working ? (
							<p className="flex items-center gap-2 text-muted-foreground">
								<Loader2 className="size-3.5 animate-spin" aria-hidden />
								{t("conversation.editFromHere.working")}
							</p>
						) : null}
						{stopped ? (
							<div
								className={cn(
									"space-y-1 rounded-md border p-3",
									stopped.resultKey.endsWith("recovery_required")
										? "border-destructive/40 bg-destructive/5 text-destructive"
										: "border-amber-500/40 bg-amber-500/5 text-amber-700 dark:text-amber-400",
								)}
								data-testid="edit-from-here-stopped"
							>
								<p className="font-medium">{t("conversation.editFromHere.stoppedTitle")}</p>
								<p>{t(stopped.resultKey)}</p>
								{stopped.reasonKey ? <p>{t(stopped.reasonKey)}</p> : null}
								{stopped.restoredTurnCount > 0 ? (
									<p>
										{t("conversation.editFromHere.stoppedRestored", {
											count: stopped.restoredTurnCount,
										})}
									</p>
								) : null}
							</div>
						) : null}
					</div>
				) : null}
				<DialogFooter className="flex-wrap gap-2">
					<Button type="button" variant="outline" disabled={working} onClick={onCancel}>
						{stopped ? t("conversation.editFromHere.close") : t("conversation.editFromHere.cancel")}
					</Button>
					{plan && !stopped && files ? (
						<>
							<Button
								type="button"
								variant={actions.includes("restore") ? "outline" : "default"}
								disabled={working}
								onClick={() => onConfirm(false)}
							>
								{newThread ? <GitFork className="size-4" /> : <Undo2 className="size-4" />}
								{actions.includes("restore")
									? newThread
										? t("conversation.editFromHere.newThread")
										: t("conversation.editFromHere.keepFiles")
									: newThread
										? t("conversation.editFromHere.newThread")
										: t("conversation.editFromHere.rewind")}
							</Button>
							{actions.includes("restore") && files.kind === "restorable" ? (
								<Button type="button" disabled={working} onClick={() => onConfirm(true)}>
									<RotateCcw className="size-4" />
									{newThread
										? t("conversation.editFromHere.newThreadRestore", {
												count: files.fileCount,
											})
										: t("conversation.editFromHere.restoreFiles", {
												count: files.fileCount,
											})}
								</Button>
							) : null}
						</>
					) : null}
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
