import { useEffect, useMemo, useState } from "react";
import { Link } from "@tanstack/react-router";
import {
	Activity,
	ChevronRight,
	Cpu,
	FolderGit2,
	Loader2,
	Plus,
	RefreshCw,
	Search,
	Settings,
	ShieldAlert,
	Smartphone,
	X,
} from "lucide-react";
import { ApiError, apiFetch } from "@/lib/api";
import { cn } from "@/lib/cn";
import { loadSession, type PairingSession } from "@/lib/session";
import { openEventStream } from "@/lib/sseClient";
import type { RawSessionEvent } from "@/lib/threadEvents";

type DaemonStatus = {
	running: boolean;
	cpuPercent: number;
	memoryMb: number;
};

type SessionSearchResult = {
	sessionId: string;
	threadTitle: string | null;
	snippet: string | null;
	providerId: string | null;
	model: string | null;
	workspaceName: string | null;
	workspaceBranch: string | null;
	workspaceId: string | null;
	projectId: string | null;
	updatedAt: string;
	archivedAt: string | null;
};

type Comb = {
	id: string;
	name: string | null;
	branch: string | null;
	projectId: string | null;
	projectName: string | null;
	status: string | null;
	lastOpenedAt: string | null;
};

type Tab = "sessions" | "workspaces";

type Bootstrap =
	| { state: "loading" }
	| { state: "unpaired" }
	| { state: "ready"; session: PairingSession };

export function HomeRoute() {
	const [boot, setBoot] = useState<Bootstrap>({ state: "loading" });

	useEffect(() => {
		void loadSession().then((s) => {
			setBoot(s ? { state: "ready", session: s } : { state: "unpaired" });
		});
	}, []);

	if (boot.state === "loading") {
		return (
			<Shell>
				<div className="flex h-[50vh] items-center justify-center text-mute">
					<Loader2 className="size-5 animate-spin" />
				</div>
			</Shell>
		);
	}
	if (boot.state === "unpaired") return <UnpairedView />;
	return <PairedHome session={boot.session} />;
}

function UnpairedView() {
	return (
		<Shell>
			<header className="px-1 pb-5">
				<div className="mb-3 inline-flex size-11 items-center justify-center rounded-xl border border-border bg-panel">
					<Smartphone className="size-5 text-mute" strokeWidth={1.8} />
				</div>
				<h1 className="text-xl font-semibold">Dev Command Center</h1>
				<p className="mt-1 text-[13px] text-mute">Mobile</p>
			</header>
			<section className="rounded-2xl border border-border bg-panel p-5">
				<h2 className="text-[14px] font-medium">Nenhum desktop pareado</h2>
				<p className="mt-1 text-[12px] leading-relaxed text-mute">
					Abra o app desktop, vá em Settings &rarr; Conexões &rarr; Parear novo
					dispositivo. Escaneie o QR com este celular.
				</p>
			</section>
		</Shell>
	);
}

function PairedHome({ session }: { session: PairingSession }) {
	const [status, setStatus] = useState<DaemonStatus | null>(null);
	const [sessions, setSessions] = useState<SessionSearchResult[] | null>(null);
	const [combs, setCombs] = useState<Comb[] | null>(null);
	const [pendingPermissions, setPendingPermissions] = useState<number>(0);
	const [error, setError] = useState<string | null>(null);
	const [refreshing, setRefreshing] = useState(false);
	const [tab, setTab] = useState<Tab>("sessions");
	const [search, setSearch] = useState("");
	const [workspaceFilter, setWorkspaceFilter] = useState<string | null>(null);

	const refresh = async () => {
		setRefreshing(true);
		setError(null);
		try {
			const [s, sess, cs] = await Promise.all([
				apiFetch<DaemonStatus>(session, "/api/v1/status").catch(() => null),
				apiFetch<SessionSearchResult[]>(
					session,
					"/api/v1/sessions/search?limit=60",
				),
				apiFetch<Comb[]>(session, "/api/v1/combs").catch(() => [] as Comb[]),
			]);
			setStatus(s);
			const visible = sess.filter((x) => !x.archivedAt);
			setSessions(visible);
			setCombs(cs);
			void countPendingPermissions(session, visible).then(setPendingPermissions);
		} catch (err) {
			if (err instanceof ApiError && err.status === 401) {
				setError("Sessão expirada. Pareie de novo no desktop.");
			} else {
				setError(err instanceof Error ? err.message : "Falha ao carregar.");
			}
		} finally {
			setRefreshing(false);
		}
	};

	useEffect(() => {
		void refresh();
		const id = window.setInterval(refresh, 15_000);
		return () => window.clearInterval(id);
	}, []);

	useEffect(() => {
		const stop = openEventStream(session, "/api/v1/events/stream", {
			onMessage: (payload) => {
				const event = payload as RawSessionEvent;
				const kind = event?.kind?.type;
				if (
					kind === "turn_permission_requested" ||
					kind === "turn_permission_resolved"
				) {
					void refresh();
				}
			},
		});
		return stop;
	}, []);

	const filteredSessions = useMemo(() => {
		if (!sessions) return null;
		const q = search.trim().toLowerCase();
		return sessions.filter((s) => {
			if (workspaceFilter && s.workspaceId !== workspaceFilter) return false;
			if (!q) return true;
			const haystack = [
				s.threadTitle,
				s.workspaceName,
				s.workspaceBranch,
				s.projectId,
				s.snippet,
			]
				.filter(Boolean)
				.join(" ")
				.toLowerCase();
			return haystack.includes(q);
		});
	}, [sessions, search, workspaceFilter]);

	const filteredCombs = useMemo(() => {
		if (!combs) return null;
		const q = search.trim().toLowerCase();
		if (!q) return combs;
		return combs.filter((c) => {
			const haystack = [c.name, c.projectName, c.projectId, c.branch]
				.filter(Boolean)
				.join(" ")
				.toLowerCase();
			return haystack.includes(q);
		});
	}, [combs, search]);

	const sessionsByWorkspace = useMemo(() => {
		const map = new Map<string, number>();
		for (const s of sessions ?? []) {
			if (s.workspaceId) {
				map.set(s.workspaceId, (map.get(s.workspaceId) ?? 0) + 1);
			}
		}
		return map;
	}, [sessions]);

	const activeWorkspace = workspaceFilter
		? combs?.find((c) => c.id === workspaceFilter) ?? null
		: null;

	return (
		<Shell>
			<header className="flex items-center justify-between gap-3 px-1 pb-5">
				<div className="min-w-0">
					<h1 className="text-xl font-semibold">Dev Command Center</h1>
					<p className="mt-0.5 break-all font-mono text-[10px] text-mute/80">
						{session.backendUrl}
					</p>
				</div>
				<div className="flex items-center gap-1">
					<Link
						to="/permissions"
						className={cn(
							"relative rounded-lg p-2",
							pendingPermissions > 0
								? "text-amber-500"
								: "text-mute hover:text-foreground",
						)}
						title="Permissões"
					>
						<ShieldAlert className="size-4" />
						{pendingPermissions > 0 ? (
							<span className="absolute -right-0.5 -top-0.5 grid min-w-[16px] place-items-center rounded-full bg-amber-500 px-1 text-[9px] font-bold text-[#241500]">
								{pendingPermissions}
							</span>
						) : null}
					</Link>
					<Link
						to="/new"
						className="rounded-lg p-2 text-mute hover:text-foreground"
						title="Nova thread"
					>
						<Plus className="size-4" />
					</Link>
					<Link
						to="/settings"
						className="-mr-1 rounded-lg p-2 text-mute hover:text-foreground"
						title="Settings"
					>
						<Settings className="size-4" />
					</Link>
				</div>
			</header>

			<StatusCard status={status} refreshing={refreshing} onRefresh={() => void refresh()} />

			{error ? (
				<p className="mt-3 rounded-xl border border-danger/30 bg-danger/5 px-4 py-3 text-[12px] text-danger">
					{error}
				</p>
			) : null}

			<section className="mt-5">
				<TabSwitcher
					tab={tab}
					onChange={(t) => {
						setTab(t);
						setWorkspaceFilter(null);
					}}
				/>

				<SearchBar value={search} onChange={setSearch} placeholder={tab === "sessions" ? "Buscar sessões…" : "Buscar workspaces…"} />

				{activeWorkspace && tab === "sessions" ? (
					<button
						type="button"
						onClick={() => setWorkspaceFilter(null)}
						className="mb-2 inline-flex items-center gap-1.5 rounded-full border border-accent/40 bg-accent/10 px-2.5 py-1 text-[11px] text-accent"
					>
						<FolderGit2 className="size-3" />
						{activeWorkspace.name ?? activeWorkspace.projectName ?? activeWorkspace.id}
						<X className="size-3" />
					</button>
				) : null}

				{tab === "sessions" ? (
					<SessionsList sessions={filteredSessions} />
				) : (
					<WorkspacesList
						combs={filteredCombs}
						sessionCount={sessionsByWorkspace}
						onPick={(id) => {
							setWorkspaceFilter(id);
							setTab("sessions");
							setSearch("");
						}}
					/>
				)}
			</section>
		</Shell>
	);
}

function TabSwitcher({ tab, onChange }: { tab: Tab; onChange: (t: Tab) => void }) {
	return (
		<div className="mb-3 flex rounded-xl border border-border bg-panel p-1 text-[12px]">
			{(["sessions", "workspaces"] as Tab[]).map((t) => (
				<button
					key={t}
					type="button"
					onClick={() => onChange(t)}
					className={cn(
						"flex-1 rounded-lg py-1.5 transition-colors",
						tab === t
							? "bg-bg text-foreground"
							: "text-mute hover:text-foreground",
					)}
				>
					{t === "sessions" ? "Sessões" : "Workspaces"}
				</button>
			))}
		</div>
	);
}

function SearchBar({
	value,
	onChange,
	placeholder,
}: {
	value: string;
	onChange: (v: string) => void;
	placeholder: string;
}) {
	return (
		<div className="mb-3 flex items-center gap-2 rounded-xl border border-border bg-panel px-3 py-1.5">
			<Search className="size-3.5 shrink-0 text-mute" />
			<input
				type="text"
				value={value}
				onChange={(e) => onChange(e.target.value)}
				placeholder={placeholder}
				className="flex-1 bg-transparent text-[13px] outline-none placeholder:text-mute/60"
			/>
			{value ? (
				<button
					type="button"
					onClick={() => onChange("")}
					className="rounded p-0.5 text-mute hover:text-foreground"
					aria-label="limpar busca"
				>
					<X className="size-3.5" />
				</button>
			) : null}
		</div>
	);
}

function StatusCard({
	status,
	refreshing,
	onRefresh,
}: {
	status: DaemonStatus | null;
	refreshing: boolean;
	onRefresh: () => void;
}) {
	return (
		<section className="rounded-2xl border border-border bg-panel p-4">
			<div className="flex items-start justify-between gap-3">
				<div className="flex items-center gap-2.5">
					<Activity
						className={cn(
							"size-4",
							status?.running ? "text-accent" : "text-mute",
						)}
						strokeWidth={2}
					/>
					<div>
						<p className="text-[13px] font-medium">
							{status?.running ? "Daemon ativo" : status ? "Daemon parado" : "—"}
						</p>
						{status ? (
							<p className="text-[11px] text-mute">
								<Cpu className="mr-1 inline size-3" />
								{status.cpuPercent.toFixed(1)}% · {status.memoryMb.toFixed(0)} MB
							</p>
						) : (
							<p className="text-[11px] text-mute">Carregando…</p>
						)}
					</div>
				</div>
				<button
					type="button"
					onClick={onRefresh}
					disabled={refreshing}
					className="rounded-lg p-1.5 text-mute hover:text-foreground disabled:opacity-50"
					title="Atualizar"
				>
					<RefreshCw className={cn("size-3.5", refreshing && "animate-spin")} />
				</button>
			</div>
		</section>
	);
}

function SessionsList({ sessions }: { sessions: SessionSearchResult[] | null }) {
	if (sessions === null) {
		return (
			<div className="rounded-2xl border border-dashed border-border/70 p-6 text-center text-[12px] text-mute">
				Carregando…
			</div>
		);
	}
	if (sessions.length === 0) {
		return (
			<div className="rounded-2xl border border-dashed border-border/70 p-6 text-center text-[12px] text-mute">
				Nenhuma sessão.
			</div>
		);
	}
	return (
		<ul className="space-y-2">
			{sessions.map((s) => (
				<li key={s.sessionId}>
					<SessionCard session={s} />
				</li>
			))}
		</ul>
	);
}

function SessionCard({ session }: { session: SessionSearchResult }) {
	const title =
		session.threadTitle?.trim() ||
		session.workspaceName ||
		session.projectId ||
		"Sessão sem título";
	const subtitle = [
		session.workspaceName ?? session.projectId,
		session.workspaceBranch,
	]
		.filter(Boolean)
		.join(" · ");
	return (
		<Link
			to="/threads/$threadId"
			params={{ threadId: session.sessionId }}
			className="flex items-start gap-3 rounded-2xl border border-border bg-panel px-4 py-3 active:bg-muted/20"
		>
			<div className="min-w-0 flex-1">
				<div className="flex items-center gap-2">
					<p className="truncate text-[14px] font-medium">{title}</p>
					<ProviderBadge providerId={session.providerId} />
				</div>
				{subtitle ? (
					<p className="mt-0.5 truncate text-[11px] text-mute">{subtitle}</p>
				) : null}
				{session.snippet ? (
					<p className="mt-1.5 line-clamp-2 text-[11px] leading-snug text-mute/80">
						{session.snippet.replace(/^User:\s*/, "")}
					</p>
				) : null}
				<p className="mt-1.5 text-[10px] uppercase tracking-wider text-mute/60">
					{formatRelative(session.updatedAt)}
				</p>
			</div>
			<ChevronRight className="mt-1 size-4 shrink-0 text-mute/60" />
		</Link>
	);
}

function WorkspacesList({
	combs,
	sessionCount,
	onPick,
}: {
	combs: Comb[] | null;
	sessionCount: Map<string, number>;
	onPick: (id: string) => void;
}) {
	if (combs === null) {
		return (
			<div className="rounded-2xl border border-dashed border-border/70 p-6 text-center text-[12px] text-mute">
				Carregando…
			</div>
		);
	}
	if (combs.length === 0) {
		return (
			<div className="rounded-2xl border border-dashed border-border/70 p-6 text-center text-[12px] text-mute">
				Nenhum workspace.
			</div>
		);
	}
	const sorted = [...combs].sort((a, b) => {
		const av = a.lastOpenedAt ?? "";
		const bv = b.lastOpenedAt ?? "";
		return bv.localeCompare(av);
	});
	return (
		<ul className="space-y-2">
			{sorted.map((c) => {
				const count = sessionCount.get(c.id) ?? 0;
				return (
					<li key={c.id}>
						<button
							type="button"
							onClick={() => onPick(c.id)}
							className="flex w-full items-start gap-3 rounded-2xl border border-border bg-panel px-4 py-3 text-left active:bg-muted/20"
						>
							<FolderGit2 className="mt-0.5 size-4 shrink-0 text-mute" />
							<div className="min-w-0 flex-1">
								<p className="truncate text-[14px] font-medium">
									{c.name ?? c.projectName ?? c.id}
								</p>
								<p className="mt-0.5 truncate text-[11px] text-mute">
									{[c.projectName, c.branch].filter(Boolean).join(" · ")}
								</p>
							</div>
							<span className="shrink-0 rounded-full border border-border bg-bg px-2 py-0.5 text-[10px] text-mute">
								{count} sessão{count === 1 ? "" : "es"}
							</span>
						</button>
					</li>
				);
			})}
		</ul>
	);
}

function ProviderBadge({ providerId }: { providerId: string | null }) {
	if (!providerId) return null;
	const label = providerId === "claude_code" ? "Claude" : providerId === "codex" ? "Codex" : providerId;
	return (
		<span className="shrink-0 rounded-full border border-border bg-bg px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wider text-mute">
			{label}
		</span>
	);
}

function formatRelative(iso: string): string {
	try {
		const date = new Date(iso);
		const diffSec = (Date.now() - date.getTime()) / 1000;
		if (diffSec < 60) return "agora";
		if (diffSec < 3600) return `${Math.floor(diffSec / 60)} min atrás`;
		if (diffSec < 86400) return `${Math.floor(diffSec / 3600)} h atrás`;
		const days = Math.floor(diffSec / 86400);
		if (days < 30) return `${days} d atrás`;
		return date.toLocaleDateString("pt-BR");
	} catch {
		return iso;
	}
}

function Shell({ children }: { children: React.ReactNode }) {
	return (
		<main className="mx-auto flex min-h-dvh max-w-md flex-col px-5 py-8">{children}</main>
	);
}

const PERMISSION_SCAN_WINDOW_MS = 48 * 3600 * 1000;

async function countPendingPermissions(
	session: PairingSession,
	threads: SessionSearchResult[],
): Promise<number> {
	const cutoff = Date.now() - PERMISSION_SCAN_WINDOW_MS;
	const recent = threads
		.filter((t) => {
			const ts = Date.parse(t.updatedAt);
			return Number.isFinite(ts) && ts > cutoff;
		})
		.slice(0, 20);
	if (recent.length === 0) return 0;
	const lists = await Promise.all(
		recent.map((thread) =>
			apiFetch<RawSessionEvent[]>(
				session,
				`/api/v1/sessions/${encodeURIComponent(thread.sessionId)}/events`,
			).catch(() => [] as RawSessionEvent[]),
		),
	);
	let count = 0;
	for (const events of lists) {
		const requested = new Set<string>();
		const resolved = new Set<string>();
		for (const ev of events) {
			const k = ev.kind;
			if (k?.type === "turn_permission_requested") {
				const id = typeof k.requestId === "string" ? k.requestId : ev.eventId;
				requested.add(id);
			} else if (k?.type === "turn_permission_resolved") {
				const id = typeof k.requestId === "string" ? k.requestId : "";
				resolved.add(id);
			}
		}
		for (const id of requested) {
			if (!resolved.has(id)) count++;
		}
	}
	return count;
}
