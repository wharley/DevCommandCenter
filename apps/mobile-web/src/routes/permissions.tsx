import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import {
	AlertTriangle,
	ArrowLeft,
	ChevronRight,
	Loader2,
	RefreshCw,
	ShieldCheck,
} from "lucide-react";
import { apiFetch, ApiError } from "@/lib/api";
import { cn } from "@/lib/cn";
import { loadSession, type PairingSession } from "@/lib/session";
import { openEventStream } from "@/lib/sseClient";
import type { RawSessionEvent } from "@/lib/threadEvents";

type SessionSearchResult = {
	sessionId: string;
	threadTitle: string | null;
	snippet: string | null;
	providerId: string | null;
	workspaceName: string | null;
	workspaceBranch: string | null;
	projectId: string | null;
	updatedAt: string;
	archivedAt: string | null;
};

type PendingPermission = {
	thread: SessionSearchResult;
	requestId: string;
	question: string;
	choices: Array<{ id: string; label: string }>;
	at: string;
};

const MAX_SESSIONS_TO_SCAN = 20;
const RECENCY_WINDOW_MS = 48 * 3600 * 1000;

export function PermissionsRoute() {
	const navigate = useNavigate();
	const [session, setSession] = useState<PairingSession | null | undefined>(undefined);
	const [pending, setPending] = useState<PendingPermission[] | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [refreshing, setRefreshing] = useState(false);
	const [resolving, setResolving] = useState<Set<string>>(new Set());

	useEffect(() => {
		void loadSession().then((s) => setSession(s));
	}, []);

	const scan = async (active: PairingSession) => {
		setRefreshing(true);
		setError(null);
		try {
			const threads = await apiFetch<SessionSearchResult[]>(
				active,
				"/api/v1/sessions/search?limit=" + MAX_SESSIONS_TO_SCAN,
			);
			// Limit scan to threads touched recently — older ones almost
			// never have unresolved permissions and they bloat the fan-out.
			const cutoff = Date.now() - RECENCY_WINDOW_MS;
			const recent = threads
				.filter((t) => !t.archivedAt)
				.filter((t) => {
					const ts = Date.parse(t.updatedAt);
					return Number.isFinite(ts) && ts > cutoff;
				});

			const eventLists = await Promise.all(
				recent.map((thread) =>
					apiFetch<RawSessionEvent[]>(
						active,
						`/api/v1/sessions/${encodeURIComponent(thread.sessionId)}/events`,
					)
						.then((events) => ({ thread, events }))
						.catch(() => ({ thread, events: [] as RawSessionEvent[] })),
				),
			);

			const result: PendingPermission[] = [];
			for (const { thread, events } of eventLists) {
				const requested = new Map<string, RawSessionEvent>();
				const resolved = new Set<string>();
				for (const event of events) {
					const kind = event.kind;
					if (kind?.type === "turn_permission_requested") {
						const id = typeof kind.requestId === "string" ? kind.requestId : event.eventId;
						requested.set(id, event);
					} else if (kind?.type === "turn_permission_resolved") {
						const id = typeof kind.requestId === "string" ? kind.requestId : "";
						resolved.add(id);
					}
				}
				for (const [id, event] of requested) {
					if (resolved.has(id)) continue;
					const kind = event.kind ?? {};
					const choices = Array.isArray(kind.choices)
						? (kind.choices as Array<{ id: string; label: string }>)
						: [
								{ id: "allow", label: "Permitir" },
								{ id: "deny", label: "Negar" },
							];
					result.push({
						thread,
						requestId: id,
						question:
							typeof kind.question === "string"
								? kind.question
								: "Pedido de permissão",
						choices,
						at: event.occurredAt,
					});
				}
			}
			// Newest first.
			result.sort((a, b) => b.at.localeCompare(a.at));
			setPending(result);
		} catch (err) {
			if (err instanceof ApiError && err.status === 401) {
				setError("Sessão expirada. Pareie novamente.");
			} else {
				setError(err instanceof Error ? err.message : "Falha ao carregar.");
			}
		} finally {
			setRefreshing(false);
		}
	};

	useEffect(() => {
		if (!session) return;
		void scan(session);
	}, [session]);

	// Live-update: when a turn_permission_* event arrives on any thread,
	// re-run the scan so the badge / list stays accurate without polling.
	useEffect(() => {
		if (!session) return;
		const stop = openEventStream(session, "/api/v1/events/stream", {
			onMessage: (payload) => {
				const event = payload as RawSessionEvent;
				const kind = event?.kind?.type;
				if (kind === "turn_permission_requested" || kind === "turn_permission_resolved") {
					void scan(session);
				}
			},
		});
		return stop;
	}, [session]);

	const respond = async (item: PendingPermission, choice: string) => {
		if (!session) return;
		const key = `${item.thread.sessionId}/${item.requestId}`;
		setResolving((prev) => new Set(prev).add(key));
		try {
			await apiFetch(
				session,
				`/api/v1/sessions/${encodeURIComponent(item.thread.sessionId)}/respond-permission`,
				{
					method: "POST",
					body: JSON.stringify({
						sessionId: item.thread.sessionId,
						requestId: item.requestId,
						choice,
					}),
				},
			);
			// Optimistically drop from the list; the SSE event will reconfirm.
			setPending((prev) =>
				prev?.filter((p) => !(p.thread.sessionId === item.thread.sessionId && p.requestId === item.requestId)) ?? prev,
			);
		} catch (err) {
			setError(err instanceof Error ? err.message : "Falha ao responder.");
		} finally {
			setResolving((prev) => {
				const next = new Set(prev);
				next.delete(key);
				return next;
			});
		}
	};

	if (session === undefined) {
		return (
			<Shell>
				<div className="flex h-[50vh] items-center justify-center text-mute">
					<Loader2 className="size-5 animate-spin" />
				</div>
			</Shell>
		);
	}

	if (session === null) {
		return (
			<Shell>
				<h1 className="text-xl font-semibold">Sem sessão</h1>
				<p className="mt-2 text-[13px] text-mute">
					Pareie o celular primeiro.{" "}
					<Link to="/" className="text-foreground underline">
						Voltar
					</Link>
				</p>
			</Shell>
		);
	}

	return (
		<Shell>
			<header className="flex items-center gap-2 pb-5">
				<button
					type="button"
					onClick={() => void navigate({ to: "/" })}
					className="-ml-2 rounded-lg p-2 text-mute hover:text-foreground"
				>
					<ArrowLeft className="size-4" />
				</button>
				<div className="min-w-0 flex-1">
					<h1 className="text-xl font-semibold">Permissões</h1>
					<p className="text-[11px] text-mute">
						{pending === null
							? "Procurando…"
							: pending.length === 0
								? "Nenhuma pendente"
								: `${pending.length} pendente${pending.length === 1 ? "" : "s"}`}
					</p>
				</div>
				<button
					type="button"
					onClick={() => void scan(session)}
					disabled={refreshing}
					className="rounded-lg p-2 text-mute hover:text-foreground disabled:opacity-50"
					title="Atualizar"
				>
					<RefreshCw className={cn("size-4", refreshing && "animate-spin")} />
				</button>
			</header>

			{error ? (
				<p className="mb-3 rounded-2xl border border-danger/30 bg-danger/5 p-3 text-[12px] text-danger">
					<AlertTriangle className="mr-1 inline size-3.5" /> {error}
				</p>
			) : null}

			<PendingList
				pending={pending}
				resolving={resolving}
				onRespond={(item, choice) => void respond(item, choice)}
			/>
		</Shell>
	);
}

function PendingList({
	pending,
	resolving,
	onRespond,
}: {
	pending: PendingPermission[] | null;
	resolving: Set<string>;
	onRespond: (item: PendingPermission, choice: string) => void;
}) {
	if (pending === null) {
		return (
			<div className="rounded-2xl border border-dashed border-border/70 p-6 text-center text-[12px] text-mute">
				Carregando…
			</div>
		);
	}
	if (pending.length === 0) {
		return (
			<div className="flex flex-col items-center gap-2 rounded-2xl border border-dashed border-border/70 p-8 text-center text-[12px] text-mute">
				<ShieldCheck className="size-6 text-accent" strokeWidth={1.6} />
				<p className="text-foreground">Tudo em dia</p>
				<p>Nenhuma sessão pedindo aprovação no momento.</p>
			</div>
		);
	}
	return (
		<ul className="space-y-3">
			{pending.map((item) => {
				const key = `${item.thread.sessionId}/${item.requestId}`;
				return (
					<li key={key}>
						<PermissionCard
							item={item}
							isResolving={resolving.has(key)}
							onRespond={(choice) => onRespond(item, choice)}
						/>
					</li>
				);
			})}
		</ul>
	);
}

function PermissionCard({
	item,
	isResolving,
	onRespond,
}: {
	item: PendingPermission;
	isResolving: boolean;
	onRespond: (choice: string) => void;
}) {
	const title =
		item.thread.threadTitle?.trim() ||
		item.thread.workspaceName ||
		item.thread.projectId ||
		"Sessão";
	const subtitle = useMemo(
		() =>
			[item.thread.workspaceName ?? item.thread.projectId, item.thread.workspaceBranch]
				.filter(Boolean)
				.join(" · "),
		[item.thread],
	);

	return (
		<div className="rounded-2xl border border-amber-500/40 bg-amber-500/5 p-3">
			<Link
				to="/threads/$threadId"
				params={{ threadId: item.thread.sessionId }}
				className="flex items-center gap-2 pb-2 text-[12px] text-foreground active:opacity-60"
			>
				<div className="min-w-0 flex-1">
					<p className="truncate font-medium">{title}</p>
					{subtitle ? (
						<p className="truncate text-[11px] text-mute">{subtitle}</p>
					) : null}
				</div>
				<ChevronRight className="size-3.5 text-mute" />
			</Link>

			<div className="flex items-start gap-2 border-t border-amber-500/20 pt-2">
				<AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-500" />
				<p className="min-w-0 flex-1 text-[13px] leading-relaxed text-foreground">
					{item.question}
				</p>
			</div>

			<div className="mt-3 flex flex-wrap gap-2">
				{item.choices.map((choice) => (
					<button
						key={choice.id}
						type="button"
						disabled={isResolving}
						onClick={() => onRespond(choice.id)}
						className={cn(
							"flex-1 rounded-lg border px-3 py-2 text-[13px] font-medium transition-opacity",
							choice.id === "deny"
								? "border-border bg-bg text-foreground active:bg-muted/30"
								: "border-accent bg-accent text-[#04231b] active:opacity-80",
							isResolving && "opacity-50",
						)}
					>
						{isResolving && choice.id !== "deny" ? (
							<span className="inline-flex items-center justify-center gap-1.5">
								<Loader2 className="size-3 animate-spin" />
								{choice.label}
							</span>
						) : (
							choice.label
						)}
					</button>
				))}
			</div>
		</div>
	);
}

function Shell({ children }: { children: React.ReactNode }) {
	return (
		<main className="mx-auto flex min-h-dvh max-w-md flex-col px-5 py-8">{children}</main>
	);
}
