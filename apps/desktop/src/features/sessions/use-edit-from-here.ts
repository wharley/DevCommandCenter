import { useCallback, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { executeConversationRewind, prepareConversationRewind } from "@/lib/session-api";
import {
	buildExecuteConversationRewindInput,
	interpretConversationRewind,
	type EditFromHereOutcome,
	type EditFromHerePlan,
} from "./edit-from-here.logic";

export type EditFromHereRequest = {
	messageId: string;
	/** Durable user turn of the message; messages without one are only copied. */
	turnId: string | null;
	prompt: string;
};

/** Queries that show files or the thread, refreshed after a rewind. */
const REFRESHED_QUERY_ROOTS = [
	"sessionThreads",
	"workspaceSessions",
	"turnReview",
	"workspaceGitStatus",
	"workspaceGitBranchDiff",
	"workspaceGitFilePreviewContent",
];

/**
 * "Edit from here" for one thread: asks the backend what a rewind would do,
 * lets the person confirm (and choose whether files are restored), then puts
 * the message back in the composer, or forks a new thread when the provider
 * cannot take its own conversation back.
 */
export function useEditFromHere({
	sessionId,
	putPromptInComposer,
	forkFromMessage,
}: {
	sessionId: string | null;
	putPromptInComposer: (prompt: string) => void;
	forkFromMessage?: (messageId: string) => void | Promise<void>;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [target, setTarget] = useState<{
		sessionId: string;
		messageId: string;
		plan: EditFromHerePlan;
	} | null>(null);
	const [working, setWorking] = useState(false);
	const [stopped, setStopped] =
		useState<Extract<EditFromHereOutcome, { kind: "files_stopped" }> | null>(null);
	const preparingRef = useRef(false);

	const begin = useCallback(
		async ({ messageId, turnId, prompt }: EditFromHereRequest) => {
			if (!sessionId || !turnId) {
				putPromptInComposer(prompt);
				return;
			}
			if (preparingRef.current) return;
			preparingRef.current = true;
			try {
				const plan = await prepareConversationRewind({ sessionId, anchorTurnId: turnId });
				if (plan.status === "blocked") {
					if (plan.reason === "anchor_not_found") {
						putPromptInComposer(prompt);
					} else {
						toast.error(t(`conversation.editFromHere.blocked.${plan.reason}`));
					}
					return;
				}
				setStopped(null);
				setTarget({ sessionId, messageId, plan });
			} catch (error) {
				toast.error(t("conversation.editFromHere.failed"), {
					description: error instanceof Error ? error.message : undefined,
				});
			} finally {
				preparingRef.current = false;
			}
		},
		[putPromptInComposer, sessionId, t],
	);

	const confirm = useCallback(
		async (restoreFiles: boolean) => {
			if (!target || working) return;
			setWorking(true);
			try {
				const outcome = interpretConversationRewind(
					await executeConversationRewind(
						buildExecuteConversationRewindInput(target.sessionId, target.plan, restoreFiles),
					),
				);
				void queryClient.invalidateQueries({
					predicate: (query) =>
						REFRESHED_QUERY_ROOTS.includes(String(query.queryKey[0] ?? "")),
				});
				switch (outcome.kind) {
					case "rewound":
						setTarget(null);
						putPromptInComposer(outcome.prompt);
						toast.success(
							outcome.restoredTurnCount > 0
								? t("conversation.editFromHere.doneRestored", {
										count: outcome.restoredTurnCount,
									})
								: t("conversation.editFromHere.done"),
						);
						break;
					case "fork":
						setTarget(null);
						await forkFromMessage?.(target.messageId);
						break;
					case "files_stopped":
						setStopped(outcome);
						break;
					case "refused":
						setTarget(null);
						toast.error(t("conversation.editFromHere.refused"));
						break;
				}
			} catch (error) {
				setTarget(null);
				toast.error(t("conversation.editFromHere.failed"), {
					description: error instanceof Error ? error.message : undefined,
				});
			} finally {
				setWorking(false);
			}
		},
		[forkFromMessage, putPromptInComposer, queryClient, t, target, working],
	);

	const cancel = useCallback(() => {
		if (working) return;
		setTarget(null);
		setStopped(null);
	}, [working]);

	return {
		begin,
		dialog: {
			plan: target?.plan ?? null,
			working,
			stopped,
			onCancel: cancel,
			onConfirm: confirm,
		},
	};
}
