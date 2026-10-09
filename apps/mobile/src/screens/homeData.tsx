/**
 * State behind both tabs: the triage scan (`@dcc/mobile-core`), the
 * workspaces, and permission answers. Refreshes when the stream (re)opens and
 * when a lifecycle event arrives.
 */
import {
	incomingEventKindType,
	runScan,
	withoutPending,
	type Comb,
	type PendingItem,
	type Scan,
} from "@dcc/mobile-core";
import { createContext, use, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { AppState } from "react-native";
import { useBackend } from "../backend/session";
import { errorMessage, isAlreadyResolved } from "../backend/types";

const LIFECYCLE = new Set([
	"turn_started",
	"turn_completed",
	"turn_aborted",
	"turn_permission_requested",
	"turn_permission_resolved",
	"turn_user_input_requested",
	"turn_user_input_resolved",
	"session_started",
	"session_completed",
	"session_aborted",
]);

type HomeData = {
	scan: Scan | null;
	combs: Comb[] | null;
	error: string | null;
	notice: string | null;
	refreshing: boolean;
	lastSynced: Date | null;
	resolving: Set<string>;
	refresh(): Promise<void>;
	respond(item: PendingItem, choice: string): Promise<void>;
	dismissNotice(): void;
};

const HomeDataContext = createContext<HomeData | null>(null);

export function pendingKey(item: PendingItem): string {
	return `${item.thread.sessionId}/${item.requestId}`;
}

export function HomeDataProvider({ children }: { children: ReactNode }) {
	const backend = useBackend();
	const [scan, setScan] = useState<Scan | null>(null);
	const [combs, setCombs] = useState<Comb[] | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [notice, setNotice] = useState<string | null>(null);
	const [refreshing, setRefreshing] = useState(false);
	const [lastSynced, setLastSynced] = useState<Date | null>(null);
	const [resolving, setResolving] = useState<Set<string>>(() => new Set());
	const running = useRef(false);
	const again = useRef(false);

	const refresh = useCallback(async () => {
		if (running.current) {
			again.current = true;
			return;
		}
		running.current = true;
		setRefreshing(true);
		try {
			do {
				again.current = false;
				const result = await runScan((path) => backend.request("GET", path));
				setScan(result.scan);
				setCombs(result.combs);
				setLastSynced(new Date());
				setError(null);
			} while (again.current);
		} catch (err) {
			setError(errorMessage(err, "Falha ao carregar."));
		} finally {
			running.current = false;
			setRefreshing(false);
		}
	}, [backend]);

	useEffect(() => {
		let timer: ReturnType<typeof setTimeout> | null = null;
		const soon = () => {
			if (timer) return;
			timer = setTimeout(() => {
				timer = null;
				void refresh();
			}, 400);
		};
		void refresh();
		const stop = backend.subscribe(() => "/api/v1/events/stream", {
			onOpen: (reopened) => {
				if (reopened) soon();
			},
			onEvent: (event) => {
				if (event.event === "resync") return soon();
				const kind = incomingEventKindType(event.data);
				if (kind && LIFECYCLE.has(kind)) soon();
			},
		});
		const appState = AppState.addEventListener("change", (state) => {
			if (state === "active") soon();
		});
		return () => {
			stop();
			appState.remove();
			if (timer) clearTimeout(timer);
		};
	}, [backend, refresh]);

	const respond = useCallback(
		async (item: PendingItem, choice: string) => {
			const key = pendingKey(item);
			setResolving((prev) => new Set(prev).add(key));
			try {
				await backend.request(
					"POST",
					`/api/v1/sessions/${encodeURIComponent(item.thread.sessionId)}/respond-permission`,
					{ sessionId: item.thread.sessionId, requestId: item.requestId, behavior: choice },
				);
				setScan((prev) => (prev ? withoutPending(prev, item.thread.sessionId, item.requestId) : prev));
			} catch (err) {
				if (isAlreadyResolved(err)) {
					setNotice("Esse pedido já tinha sido respondido em outro lugar.");
					setScan((prev) => (prev ? withoutPending(prev, item.thread.sessionId, item.requestId) : prev));
				} else {
					setError(errorMessage(err, "Falha ao responder."));
				}
			} finally {
				setResolving((prev) => {
					const next = new Set(prev);
					next.delete(key);
					return next;
				});
			}
		},
		[backend],
	);

	const value = useMemo<HomeData>(
		() => ({
			scan,
			combs,
			error,
			notice,
			refreshing,
			lastSynced,
			resolving,
			refresh,
			respond,
			dismissNotice: () => setNotice(null),
		}),
		[scan, combs, error, notice, refreshing, lastSynced, resolving, refresh, respond],
	);
	return <HomeDataContext value={value}>{children}</HomeDataContext>;
}

export function useHomeData(): HomeData {
	const value = use(HomeDataContext);
	if (!value) throw new Error("useHomeData outside HomeDataProvider");
	return value;
}
