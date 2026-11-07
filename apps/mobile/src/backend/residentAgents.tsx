/**
 * The resident agents for the home's "Agentes" (`@dcc/mobile-core`
 * residentAgents): read on open, after the stream's lifecycle events (a
 * turn started or ended), on reconnect, when the app comes back to the
 * front and once a minute while it is up — always sending back the
 * `snapshotId` held, so an unchanged answer is a few bytes over the relay.
 */
import { parseResidentAgents, type ResidentAgent } from "@dcc/mobile-core";
import { createContext, use, useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { AppState } from "react-native";
import { useSession } from "./session";
import type { Backend } from "./types";

const POLL_MS = 60_000;
/** Lifecycle events come in bursts: one read after them. */
const SETTLE_MS = 600;

const ResidentAgentsContext = createContext<ResidentAgent[]>([]);

export function ResidentAgentsProvider({ children }: { children: ReactNode }) {
	const { session } = useSession();
	if (session.status !== "remote" && session.status !== "demo") return children;
	return <Live backend={session.backend}>{children}</Live>;
}

function Live({ backend, children }: { backend: Backend; children: ReactNode }) {
	const [agents, setAgents] = useState<ResidentAgent[]>([]);
	const snapshot = useRef<string | null>(null);
	const loading = useRef(false);
	const settle = useRef<ReturnType<typeof setTimeout> | null>(null);

	const refresh = useCallback(() => {
		if (loading.current) return;
		loading.current = true;
		const after = snapshot.current;
		const query = after ? `?after=${encodeURIComponent(after)}` : "";
		backend
			.request("GET", `/api/v1/mobile/agents${query}`)
			.then((data) => {
				const read = parseResidentAgents(data, after);
				if (!read) {
					snapshot.current = null;
					return;
				}
				snapshot.current = read.snapshotId;
				if (!read.unchanged) setAgents(read.agents);
			})
			// Older desktops have no route: no agents to show.
			.catch(() => {})
			.finally(() => {
				loading.current = false;
			});
	}, [backend]);

	useEffect(() => {
		refresh();
		const later = () => {
			if (settle.current) clearTimeout(settle.current);
			settle.current = setTimeout(refresh, SETTLE_MS);
		};
		const stop = backend.subscribe(() => "/api/v1/events/stream?kinds=lifecycle", {
			onOpen: (reopened) => {
				if (reopened) refresh();
			},
			onEvent: () => later(),
		});
		const poll = setInterval(() => {
			if (AppState.currentState === "active") refresh();
		}, POLL_MS);
		const appState = AppState.addEventListener("change", (state) => {
			if (state === "active") refresh();
		});
		return () => {
			stop();
			clearInterval(poll);
			if (settle.current) clearTimeout(settle.current);
			appState.remove();
		};
	}, [backend, refresh]);

	return <ResidentAgentsContext value={agents}>{children}</ResidentAgentsContext>;
}

export function useResidentAgents(): ResidentAgent[] {
	return use(ResidentAgentsContext);
}
