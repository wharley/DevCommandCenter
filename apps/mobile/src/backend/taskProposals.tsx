/**
 * Tasks agents proposed (`@dcc/mobile-core` taskProposals), for the home's
 * "Precisa de você" and the thread that proposed them. Fetched on open, on
 * every delegation event of the stream, on reconnect and when the app comes
 * back to the front.
 */
import { isDelegationStreamEvent, parseTaskProposals, type TaskProposal } from "@dcc/mobile-core";
import { createContext, use, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { AppState } from "react-native";
import { useSession } from "./session";
import { ApiError, errorMessage, type Backend } from "./types";

export type TaskProposalAnswer = "start" | "dismiss";

type TaskProposalsValue = {
	proposals: TaskProposal[];
	answering: Set<string>;
	/** Resolves to a notice for the person (someone answered first), or null. */
	answer(proposal: TaskProposal, action: TaskProposalAnswer): Promise<string | null>;
};

const EMPTY: TaskProposalsValue = {
	proposals: [],
	answering: new Set(),
	answer: async () => null,
};

const TaskProposalsContext = createContext<TaskProposalsValue>(EMPTY);

export function TaskProposalsProvider({ children }: { children: ReactNode }) {
	const { session } = useSession();
	if (session.status !== "remote" && session.status !== "demo") return children;
	return <Live backend={session.backend}>{children}</Live>;
}

function Live({ backend, children }: { backend: Backend; children: ReactNode }) {
	const [proposals, setProposals] = useState<TaskProposal[]>([]);
	const [answering, setAnswering] = useState<Set<string>>(() => new Set());
	const loading = useRef(false);

	const refresh = useCallback(() => {
		if (loading.current) return;
		loading.current = true;
		backend
			.request("GET", "/api/v1/mobile/task-proposals")
			.then((data) => setProposals(parseTaskProposals(data)))
			// Older desktops have no route: nothing to show.
			.catch(() => {})
			.finally(() => {
				loading.current = false;
			});
	}, [backend]);

	useEffect(() => {
		refresh();
		const stop = backend.subscribe(() => "/api/v1/events/stream", {
			onOpen: (reopened) => {
				if (reopened) refresh();
			},
			onEvent: (event) => {
				if (event.event === "resync" || isDelegationStreamEvent(event.data)) refresh();
			},
		});
		const appState = AppState.addEventListener("change", (state) => {
			if (state === "active") refresh();
		});
		return () => {
			stop();
			appState.remove();
		};
	}, [backend, refresh]);

	const answer = useCallback(
		async (proposal: TaskProposal, action: TaskProposalAnswer) => {
			setAnswering((prev) => new Set(prev).add(proposal.id));
			const drop = () => setProposals((prev) => prev.filter((p) => p.id !== proposal.id));
			try {
				await backend.request(
					"POST",
					`/api/v1/mobile/task-proposals/${encodeURIComponent(proposal.id)}/${action}`,
				);
				drop();
				return null;
			} catch (error) {
				if (error instanceof ApiError && (error.status === 409 || error.status === 404)) {
					drop();
					return error.status === 409
						? "Essa proposta já foi respondida."
						: "Essa proposta não está mais aberta.";
				}
				return errorMessage(error, action === "start" ? "Falha ao começar a tarefa." : "Falha ao descartar.");
			} finally {
				setAnswering((prev) => {
					const next = new Set(prev);
					next.delete(proposal.id);
					return next;
				});
				refresh();
			}
		},
		[backend, refresh],
	);

	const value = useMemo(() => ({ proposals, answering, answer }), [proposals, answering, answer]);
	return <TaskProposalsContext value={value}>{children}</TaskProposalsContext>;
}

export function useTaskProposals(): TaskProposalsValue {
	return use(TaskProposalsContext);
}
