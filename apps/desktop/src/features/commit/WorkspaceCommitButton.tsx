import { Check, Loader2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import type { WorkspaceCommitMessageSuggestion } from "./commit-message";
import type { CommitButtonStatus, CommitMode } from "./WorkspaceCommitButton.logic";
import { commitModeClassName, commitTranslationKey } from "./WorkspaceCommitButton.logic";

type WorkspaceCommitButtonProps = {
	mode: CommitMode;
	prProvider?: string | null;
	disabled?: boolean;
	onCommit?: (
		message?: string,
		body?: string | null,
		stagedFingerprint?: string,
	) => Promise<void> | void;
	onPrepareCommitMessage?: () => Promise<WorkspaceCommitMessageSuggestion>;
};

export function WorkspaceCommitButton({
	mode,
	prProvider = null,
	disabled = false,
	onCommit,
	onPrepareCommitMessage,
}: WorkspaceCommitButtonProps) {
	const { t } = useTranslation("common");
	const [status, setStatus] = useState<CommitButtonStatus>("idle");
	const [previewOpen, setPreviewOpen] = useState(false);
	const [preview, setPreview] = useState<WorkspaceCommitMessageSuggestion | null>(null);
	const [message, setMessage] = useState("");
	const [body, setBody] = useState("");
	const requestLabel = prProvider === "gitlab" ? "MR" : "PR";
	const label = useMemo(
		() => t(commitTranslationKey(mode, status), { requestLabel }),
		[mode, requestLabel, status, t],
	);
	const isLocked = disabled || mode === "merged" || mode === "closed";

	useEffect(() => {
		if (status !== "done" && status !== "error") {
			return;
		}

		const timeout = window.setTimeout(() => setStatus("idle"), status === "done" ? 900 : 1200);
		return () => window.clearTimeout(timeout);
	}, [status]);

	return (
		<>
			<Button
				type="button"
				variant="outline"
				size="sm"
				className={cn(
					"h-8 gap-1.5 rounded-[9px] px-2.5 text-[12px] font-medium",
					commitModeClassName(mode),
				)}
				disabled={isLocked || status === "busy"}
				onClick={async () => {
					if (isLocked || status === "busy") {
						return;
					}

					setStatus("busy");
					try {
						if ((mode === "commit" || mode === "commit-and-push") && onPrepareCommitMessage) {
							const suggestion = await onPrepareCommitMessage();
							setPreview(suggestion);
							setMessage(suggestion.subject);
							setBody(suggestion.body ?? "");
							setPreviewOpen(true);
							setStatus("idle");
							return;
						}
						await onCommit?.();
						setStatus("done");
					} catch {
						setStatus("error");
					}
				}}
			>
				{status === "busy" ? (
					<Loader2 className="size-3.5 animate-spin" />
				) : status === "done" ? (
					<Check className="size-3.5" />
				) : null}
				{label}
			</Button>
			<Dialog
				open={previewOpen}
				onOpenChange={(open) => !status.includes("busy") && setPreviewOpen(open)}
			>
				<DialogContent className="sm:max-w-lg">
					<DialogHeader>
						<DialogTitle>{t("commit.preview.title")}</DialogTitle>
						<DialogDescription>{t("commit.preview.description")}</DialogDescription>
					</DialogHeader>
					<div className="grid gap-2 py-3">
						<label className="text-[12px] font-medium" htmlFor="commit-message-preview">
							{t("commit.preview.label")}
						</label>
						<Textarea
							id="commit-message-preview"
							value={message}
							onChange={(event) => setMessage(event.target.value)}
							className="min-h-24 resize-y font-mono text-[12px]"
							autoFocus
						/>
						<label className="text-[12px] font-medium" htmlFor="commit-message-body-preview">
							{t("commit.preview.bodyLabel")}
						</label>
						<Textarea
							id="commit-message-body-preview"
							value={body}
							onChange={(event) => setBody(event.target.value)}
							className="min-h-20 resize-y font-mono text-[12px]"
						/>
						{preview ? (
							<p className="text-[11px] text-muted-foreground">
								{t("commit.preview.source", { count: preview.stagedFileCount ?? 0 })}
							</p>
						) : null}
					</div>
					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							onClick={() => setPreviewOpen(false)}
						>
							{t("commit.preview.cancel")}
						</Button>
						<Button
							type="button"
							disabled={!message.trim()}
							onClick={async () => {
								setStatus("busy");
								try {
									await onCommit?.(message, body, preview?.stagedFingerprint);
									setPreviewOpen(false);
									setStatus("done");
								} catch {
									setStatus("error");
								}
							}}
						>
							{t("commit.preview.confirm")}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</>
	);
}
