import { Check, Loader2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { CommitButtonStatus, CommitMode } from "./WorkspaceCommitButton.logic";
import { commitModeClassName, commitTranslationKey } from "./WorkspaceCommitButton.logic";

type WorkspaceCommitButtonProps = {
	mode: CommitMode;
	disabled?: boolean;
	onCommit?: () => Promise<void> | void;
};

export function WorkspaceCommitButton({
	mode,
	disabled = false,
	onCommit,
}: WorkspaceCommitButtonProps) {
	const { t } = useTranslation("common");
	const [status, setStatus] = useState<CommitButtonStatus>("idle");
	const label = useMemo(
		() => t(commitTranslationKey(mode, status)),
		[mode, status, t],
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
	);
}
