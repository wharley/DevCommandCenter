/**
 * Browser and computer-use requests from agents (`@dcc/mobile-core`
 * agentRequests), for the home's "Precisa de você" and the thread. One list
 * for the whole app: fetched on open, on every `agent-requests` event of the
 * stream, on reconnect and when the app comes back to the front.
 */
import {
	conflictOutcome,
	openAgentRequests,
	outcomeText,
	parseAgentRequests,
	type AgentRequest,
} from "@dcc/mobile-core";
import { createContext, use, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { AppState } from "react-native";
import { useSession } from "./session";
import { ApiError, errorMessage, type Backend } from "./types";

export type AgentRequestAnswer = { allow: boolean; allowedBundleIds?: string[] };

type AgentRequestsValue = {
	requests: AgentRequest[];
	answering: Set<string>;
	/** Resolves to a notice for the person (a 409 says who answered first), or null. */
	respond(request: AgentRequest, answer: AgentRequestAnswer): Promise<string | null>;
	refresh(): void;
};

const EMPTY: AgentRequestsValue = {
	requests: [],
	answering: new Set(),
	respond: async () => null,
	refresh: () => {},
};

const AgentRequestsContext = createContext<AgentRequestsValue>(EMPTY);

export function AgentRequestsProvider({ children }: { children: ReactNode }) {
	const { session } = useSession();
	if (session.status !== "remote" && session.status !== "demo") return children;
	return <Live backend={session.backend}>{children}</Live>;
}

function Live({ backend, children }: { backend: Backend; children: ReactNode }) {
	const [requests, setRequests] = useState<AgentRequest[]>([]);
	const [answering, setAnswering] = useState<Set<string>>(() => new Set());
	const [now, setNow] = useState(() => Date.now());
	const loading = useRef(false);

	const refresh = useCallback(() => {
		if (loading.current) return;
		loading.current = true;
		backend
			.request("GET", "/api/v1/mobile/agent-requests")
			.then((data) => setRequests(parseAgentRequests(data)))
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
				// The stream's list has no computer-use details; fetch them.
				if (event.event === "agent-requests") refresh();
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

	// Requests expire within a minute; drop them when they do.
	useEffect(() => {
		if (requests.length === 0) return;
		const timer = setInterval(() => setNow(Date.now()), 1000);
		return () => clearInterval(timer);
	}, [requests.length]);

	const respond = useCallback(
		async (request: AgentRequest, answer: AgentRequestAnswer) => {
			setAnswering((prev) => new Set(prev).add(request.requestId));
			const drop = () => setRequests((prev) => prev.filter((r) => r.requestId !== request.requestId));
			try {
				await backend.request(
					"POST",
					`/api/v1/mobile/agent-requests/${encodeURIComponent(request.requestId)}/respond`,
					answer,
				);
				drop();
				return null;
			} catch (error) {
				if (error instanceof ApiError && (error.status === 409 || error.status === 404)) {
					drop();
					return error.status === 409 ? outcomeText(conflictOutcome(error.body)) : "Esse pedido não está mais aberto.";
				}
				return errorMessage(error, "Falha ao responder.");
			} finally {
				setAnswering((prev) => {
					const next = new Set(prev);
					next.delete(request.requestId);
					return next;
				});
				refresh();
			}
		},
		[backend, refresh],
	);

	const open = useMemo(() => openAgentRequests(requests, now), [requests, now]);
	const value = useMemo(() => ({ requests: open, answering, respond, refresh }), [open, answering, respond, refresh]);
	return <AgentRequestsContext value={value}>{children}</AgentRequestsContext>;
}

export function useAgentRequests(): AgentRequestsValue {
	return use(AgentRequestsContext);
}
