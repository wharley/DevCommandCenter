/**
 * State behind the home and its project sheet: the triage scan (`@dcc/mobile-core`), the
 * workspaces, the desktop's catalog (projects, agents) and permission
 * answers.
 *
 * What runs and what waits comes from the desktop's own state
 * (`/api/v1/mobile/attention`, the sidebar's rule), never rebuilt from
 * histories — the way Orca's phone reads `worktree.ps`. It is read again when
 * a lifecycle event arrives, on (re)connect and foreground, and every few
 * seconds as a safety net that costs a few bytes while nothing changed (the
 * `snapshotId` round trip). Between reads, lifecycle events move a task in or
 * out of "Rodando" right away. Older desktops fall back to the history sweep.
 * The catalog — which asks each agent runtime for its models — only on open,
 * reconnect, foreground and pull-to-refresh.
 */
import {
	incomingEventKindType,
	incomingEventSessionId,
	liveRunning,
	needsYouEntries,
	parseHomeSnapshot,
	runScan,
	scanFromSnapshot,
	withLiveOverlay,
	withoutPending,
	type AttentionEntry,
	type Comb,
	type MobileCatalog,
	type PendingItem,
	type Scan,
	type SessionSearchResult,
} from "@dcc/mobile-core";
import { createContext, use, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { AppState } from "react-native";
import { cachedCatalog, fetchCatalog } from "../backend/catalogCache";
import { loadScope, saveScope } from "../backend/scopeStore";
import { loadSeenResults, onSeenResults } from "../backend/seenResults";
import { useBackend } from "../backend/session";
import { ApiError, errorMessage, isAlreadyResolved } from "../backend/types";

/** The safety-net read while the app is in front (an `unchanged` answer when nothing moved). */
const POLL_MS = 10_000;

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
	/**
	 * Every task's reason to be under "Precisa de você", the desktop's rule
	 * and order, minus results already opened here. Null on a desktop
	 * without the route (the scan's permissions/questions stand in).
	 */
	attention: AttentionEntry[] | null;
	combs: Comb[] | null;
	/** Null until loaded, or when this desktop has no catalog route. */
	catalog: MobileCatalog | null;
	error: string | null;
	notice: string | null;
	refreshing: boolean;
	lastSynced: Date | null;
	resolving: Set<string>;
	/** The project the home shows (`ProjectGroup.key`); null is every project. */
	scopeKey: string | null;
	setScope(projectKey: string | null): void;
	refresh(): Promise<void>;
	respond(item: PendingItem, choice: string): Promise<void>;
	showNotice(notice: string): void;
	dismissNotice(): void;
};

const HomeDataContext = createContext<HomeData | null>(null);

export function pendingKey(item: PendingItem): string {
	return `${item.thread.sessionId}/${item.requestId}`;
}

export function HomeDataProvider({ children }: { children: ReactNode }) {
	const backend = useBackend();
	const [baseScan, setBaseScan] = useState<Scan | null>(null);
	const [overlay, setOverlay] = useState<Map<string, { running: boolean; seq: number }>>(() => new Map());
	const [rawAttention, setRawAttention] = useState<AttentionEntry[] | null>(null);
	const [seen, setSeen] = useState<Awaited<ReturnType<typeof loadSeenResults>> | null>(null);
	const [combs, setCombs] = useState<Comb[] | null>(null);
	const [catalog, setCatalog] = useState<MobileCatalog | null>(() => cachedCatalog(backend));
	const [error, setError] = useState<string | null>(null);
	const [notice, setNotice] = useState<string | null>(null);
	const [refreshing, setRefreshing] = useState(false);
	const [lastSynced, setLastSynced] = useState<Date | null>(null);
	const [resolving, setResolving] = useState<Set<string>>(() => new Set());
	const [scopeKey, setScopeKey] = useState<string | null>(null);
	const running = useRef(false);
	const again = useRef(false);
	const scanRef = useRef<Scan | null>(null);
	/** The desktop's last `snapshotId`; null on desktops that do not send one. */
	const snapshotId = useRef<string | null>(null);
	/** Counts lifecycle events, so a read drops only the overlay entries it already saw. */
	const eventSeq = useRef(0);
	const overlayRef = useRef(overlay);

	const commitScan = useCallback((next: Scan) => {
		scanRef.current = next;
		setBaseScan(next);
	}, []);

	/** A read that started after event `seq` already reflects it. */
	const settleOverlay = useCallback((seq: number) => {
		const next = new Map([...overlayRef.current].filter(([, entry]) => entry.seq > seq));
		if (next.size === overlayRef.current.size) return;
		overlayRef.current = next;
		setOverlay(next);
	}, []);

	/**
	 * `poll` asks only whether the desktop's state moved (`after=<snapshotId>`)
	 * and reads the rest when it did; `full` reads everything. A full request
	 * during a read runs once more after it; a poll during a read is dropped.
	 */
	const read = useCallback(
		async (mode: "full" | "poll") => {
			if (running.current) {
				if (mode === "full") again.current = true;
				return;
			}
			running.current = true;
			if (mode === "full") setRefreshing(true);
			try {
				let asking = mode;
				do {
					again.current = false;
					const startedAt = eventSeq.current;
					const after = asking === "poll" ? snapshotId.current : null;
					const raw = await backend
						.request("GET", after ? `/api/v1/mobile/attention?after=${encodeURIComponent(after)}` : "/api/v1/mobile/attention")
						// Older desktops have no route: the scan's permissions/questions stand in.
						// Any other failure keeps what is on screen (the catch below).
						.catch((err: unknown) => {
							if (err instanceof ApiError && err.status === 404) return null;
							throw err;
						});
					const snapshot = raw === null ? null : parseHomeSnapshot(raw, after);
					if (snapshot?.unchanged) {
						snapshotId.current = snapshot.snapshotId;
					} else if (snapshot && snapshot.running && snapshot.requests) {
						const [sessions, nextCombs] = await Promise.all([
							backend.request<SessionSearchResult[]>("GET", "/api/v1/sessions/search?limit=60"),
							backend.request<Comb[]>("GET", "/api/v1/combs").catch(() => [] as Comb[]),
						]);
						commitScan(scanFromSnapshot(sessions, nextCombs, snapshot.running, snapshot.requests));
						setCombs(nextCombs);
						setRawAttention(snapshot.entries);
						snapshotId.current = snapshot.snapshotId;
					} else {
						const result = await runScan((path) => backend.request("GET", path), scanRef.current);
						commitScan(result.scan);
						setCombs(result.combs);
						setRawAttention(snapshot ? snapshot.entries : null);
						snapshotId.current = null;
					}
					settleOverlay(startedAt);
					setLastSynced(new Date());
					setError(null);
					asking = "full";
				} while (again.current);
			} catch (err) {
				// What was shown stays; the banner says the read failed.
				setError(errorMessage(err, "Falha ao carregar."));
			} finally {
				running.current = false;
				setRefreshing(false);
			}
		},
		[backend, commitScan, settleOverlay],
	);

	const refresh = useCallback(() => read("full"), [read]);

	const loadCatalog = useCallback(async () => {
		try {
			setCatalog(await fetchCatalog(backend));
		} catch {
			// Older desktops: the project sheet falls back to the projects seen through workspaces.
		}
	}, [backend]);

	const refreshAll = useCallback(async () => {
		await Promise.all([refresh(), loadCatalog()]);
	}, [refresh, loadCatalog]);

	useEffect(() => {
		let cancelled = false;
		void loadSeenResults().then((value) => {
			if (!cancelled) setSeen(value);
		});
		const stop = onSeenResults(setSeen);
		return () => {
			cancelled = true;
			stop();
		};
	}, []);

	useEffect(() => {
		let cancelled = false;
		void loadScope().then((key) => {
			if (!cancelled) setScopeKey((current) => current ?? key);
		});
		return () => {
			cancelled = true;
		};
	}, []);

	const setScope = useCallback((projectKey: string | null) => {
		setScopeKey(projectKey);
		saveScope(projectKey);
	}, []);

	const attention = useMemo(
		() => (rawAttention && seen ? needsYouEntries(rawAttention, seen.seen, seen.baseline) : null),
		[rawAttention, seen],
	);

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
		void loadCatalog();
		const stop = backend.subscribe(() => "/api/v1/events/stream?kinds=lifecycle", {
			onOpen: (reopened) => {
				if (reopened) {
					soon();
					void loadCatalog();
				}
			},
			onEvent: (event) => {
				if (event.event === "resync") return soon();
				const kind = incomingEventKindType(event.data);
				if (!kind || !LIFECYCLE.has(kind)) return;
				const sessionId = incomingEventSessionId(event.data);
				const live = liveRunning(kind);
				if (sessionId && live !== null) {
					eventSeq.current += 1;
					const next = new Map(overlayRef.current).set(sessionId, { running: live, seq: eventSeq.current });
					overlayRef.current = next;
					setOverlay(next);
				}
				soon();
			},
		});
		// Only desktops that name their state get polled: elsewhere a poll is a full sweep.
		const poll = setInterval(() => {
			if (AppState.currentState === "active" && snapshotId.current) void read("poll");
		}, POLL_MS);
		const appState = AppState.addEventListener("change", (state) => {
			if (state === "active") {
				soon();
				void loadCatalog();
			}
		});
		return () => {
			stop();
			clearInterval(poll);
			appState.remove();
			if (timer) clearTimeout(timer);
		};
	}, [backend, refresh, read, loadCatalog]);

	const scan = useMemo(() => (baseScan ? withLiveOverlay(baseScan, overlay) : null), [baseScan, overlay]);
	const setScan = useCallback(
		(update: (prev: Scan | null) => Scan | null) => {
			const next = update(scanRef.current);
			if (next) commitScan(next);
		},
		[commitScan],
	);

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
		[backend, setScan],
	);

	const value = useMemo<HomeData>(
		() => ({
			scan,
			attention,
			combs,
			catalog,
			error,
			notice,
			refreshing,
			lastSynced,
			resolving,
			scopeKey,
			setScope,
			refresh: refreshAll,
			respond,
			showNotice: setNotice,
			dismissNotice: () => setNotice(null),
		}),
		[scan, attention, combs, catalog, error, notice, refreshing, lastSynced, resolving, scopeKey, setScope, refreshAll, respond],
	);
	return <HomeDataContext value={value}>{children}</HomeDataContext>;
}

export function useHomeData(): HomeData {
	const value = use(HomeDataContext);
	if (!value) throw new Error("useHomeData outside HomeDataProvider");
	return value;
}
