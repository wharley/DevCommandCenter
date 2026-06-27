import { useEffect, useMemo, useState } from "react";
import { Link } from "@tanstack/react-router";
import {
	ChevronRight,
	Cpu,
	FolderGit2,
	GitBranch,
	Inbox,
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
import { indexBundle, type BundleEntry, type WorktreeDiff } from "@/lib/diff";
import {
	Rest,
	SectionLabel,
	Shell,
	StateDot,
	Wordmark,
	type AgentState,
} from "@/components/ui";

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
	worktreePath: string | null;
	status: string | null;
	lastOpenedAt: string | null;
};

type PendingItem = {
	thread: SessionSearchResult;
	requestId: string;
	question: string;
	choices: Array<{ id: string; label: string }>;
	at: string;
};

type Scan = {
	sessions: SessionSearchResult[];
	/** sessionId → derived live state from a recent-events sweep. */
	running: Set<string>;
	needsYou: PendingItem[];
};

type Tab = "agents" | "workspaces";

type Bootstrap =
	| { state: "loading" }
	| { state: "unpaired" }
	| { state: "ready"; session: PairingSession };

const RECENT_SCAN_WINDOW_MS = 48 * 3600 * 1000;
const RECENT_SCAN_LIMIT = 20;

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
			<header className="px-0.5 pb-6">
				<Wordmark />
			</header>
			<section className="rounded-2xl border border-border bg-panel p-6">
				<div className="mb-4 inline-flex size-11 items-center justify-center rounded-xl border border-border bg-elevated">
					<Smartphone className="size-5 text-mute" strokeWidth={1.8} />
				</div>
				<h2 className="text-[15px] font-semibold">Conecte um desktop</h2>
				<p className="mt-1.5 text-[13px] leading-relaxed text-mute">
					No app desktop, abra Settings &rarr; Conexões &rarr; Parear novo
					dispositivo e escaneie o QR com este celular.
				</p>
			</section>
		</Shell>
	);
}

function PairedHome({ session }: { session: PairingSession }) {
	const [status, setStatus] = useState<DaemonStatus | null>(null);
	const [scan, setScan] = useState<Scan | null>(null);
	const [combs, setCombs] = useState<Comb[] | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [refreshing, setRefreshing] = useState(false);
	const [tab, setTab] = useState<Tab>("agents");
	const [search, setSearch] = useState("");
	const [workspaceFilter, setWorkspaceFilter] = useState<string | null>(null);
	const [resolving, setResolving] = useState<Set<string>>(new Set());
	const [diffs, setDiffs] = useState<Map<string, WorktreeDiff> | null>(null);

	const refresh = async () => {
		setRefreshing(true);
		setError(null);
		try {
			const [s, result] = await Promise.all([
				apiFetch<DaemonStatus>(session, "/api/v1/status").catch(() => null),
				runScan(session),
			]);
			setStatus(s);
			setScan(result.scan);
			setCombs(result.combs);
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
					kind === "turn_permission_resolved" ||
					kind === "turn_completed" ||
					kind === "turn_started"
				) {
					void refresh();
				}
			},
		});
		return stop;
	}, []);

	// Lazy: only pull worktree diffs while the Workspaces tab is open. One
	// batched call covers every comb. Re-runs as `combs` refreshes (~15s) so
	// the +/- pills stay current without nulling (no spinner flicker).
	useEffect(() => {
		if (tab !== "workspaces" || !combs || combs.length === 0) return;
		let cancelled = false;
		const ids = combs.map((c) => c.id);
		apiFetch<BundleEntry[]>(session, "/api/v1/diffs/bundle", {
			method: "POST",
			body: JSON.stringify({ combIds: ids, worktreePaths: [] }),
		})
			.then((bundle) => {
				if (!cancelled) setDiffs(indexBundle(bundle));
			})
			.catch(() => {
				if (!cancelled) setDiffs((prev) => prev ?? new Map());
			});
		return () => {
			cancelled = true;
		};
	}, [tab, combs, session]);

	const respond = async (item: PendingItem, choice: string) => {
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
			setScan((prev) =>
				prev
					? {
							...prev,
							needsYou: prev.needsYou.filter(
								(p) =>
									!(
										p.thread.sessionId === item.thread.sessionId &&
										p.requestId === item.requestId
									),
							),
						}
					: prev,
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

	const sessions = scan?.sessions ?? null;

	const flatSearch = useMemo(() => {
		if (!sessions) return null;
		const q = search.trim().toLowerCase();
		if (!q) return null;
		return sessions.filter((s) => {
			if (workspaceFilter && s.workspaceId !== workspaceFilter) return false;
			return [s.threadTitle, s.workspaceName, s.workspaceBranch, s.projectId, s.snippet]
				.filter(Boolean)
				.join(" ")
				.toLowerCase()
				.includes(q);
		});
	}, [sessions, search, workspaceFilter]);

	const groups = useMemo(() => {
		if (!scan) return null;
		const needsYou = workspaceFilter
			? scan.needsYou.filter((p) => p.thread.workspaceId === workspaceFilter)
			: scan.needsYou;
		const blocked = new Set(needsYou.map((p) => p.thread.sessionId));
		const visible = scan.sessions.filter(
			(s) => !workspaceFilter || s.workspaceId === workspaceFilter,
		);
		const running = visible.filter(
			(s) => scan.running.has(s.sessionId) && !blocked.has(s.sessionId),
		);
		const runningIds = new Set(running.map((s) => s.sessionId));
		const recent = visible
			.filter((s) => !blocked.has(s.sessionId) && !runningIds.has(s.sessionId))
			.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
		return { needsYou, running, recent };
	}, [scan, workspaceFilter]);

	const sessionsByWorkspace = useMemo(() => {
		const map = new Map<string, number>();
		for (const s of sessions ?? []) {
			if (s.workspaceId)
				map.set(s.workspaceId, (map.get(s.workspaceId) ?? 0) + 1);
		}
		return map;
	}, [sessions]);

	const filteredCombs = useMemo(() => {
		if (!combs) return null;
		const q = search.trim().toLowerCase();
		const base = q
			? combs.filter((c) =>
					[c.name, c.projectName, c.projectId, c.branch]
						.filter(Boolean)
						.join(" ")
						.toLowerCase()
						.includes(q),
				)
			: combs;
		return [...base].sort((a, b) =>
			(b.lastOpenedAt ?? "").localeCompare(a.lastOpenedAt ?? ""),
		);
	}, [combs, search]);

	const activeWorkspace = workspaceFilter
		? combs?.find((c) => c.id === workspaceFilter) ?? null
		: null;
	const needsYouCount = scan?.needsYou.length ?? 0;

	return (
		<Shell>
			<header className="flex items-center justify-between gap-2 pb-5">
				<Wordmark online={status?.running} />
				<div className="-mr-1 flex items-center">
					<Link
						to="/permissions"
						title="Permissões"
						aria-label="Permissões"
						className={cn(
							"relative grid size-9 place-items-center rounded-xl transition-colors active:bg-elevated",
							needsYouCount > 0 ? "text-wait" : "text-mute hover:text-foreground",
						)}
					>
						<ShieldAlert className="size-[18px]" />
						{needsYouCount > 0 ? (
							<span className="absolute -right-0.5 -top-0.5 grid min-w-[16px] place-items-center rounded-full bg-wait px-1 font-mono text-[9px] font-bold text-[var(--color-wait-ink)]">
								{needsYouCount}
							</span>
						) : null}
					</Link>
					<Link
						to="/new"
						title="Nova thread"
						aria-label="Nova thread"
						className="grid size-9 place-items-center rounded-xl text-mute transition-colors hover:text-foreground active:bg-elevated"
					>
						<Plus className="size-[18px]" />
					</Link>
					<Link
						to="/settings"
						title="Settings"
						aria-label="Settings"
						className="grid size-9 place-items-center rounded-xl text-mute transition-colors hover:text-foreground active:bg-elevated"
					>
						<Settings className="size-[18px]" />
					</Link>
				</div>
			</header>

			<DaemonBar status={status} refreshing={refreshing} onRefresh={() => void refresh()} />

			{error ? (
				<p className="mt-3 rounded-xl border border-danger/30 bg-danger/5 px-4 py-3 text-[12px] text-danger">
					{error}
				</p>
			) : null}

			<div className="mt-5">
				<Segmented tab={tab} onChange={(t) => {
					setTab(t);
					setWorkspaceFilter(null);
					setSearch("");
				}} />

				<SearchBar
					value={search}
					onChange={setSearch}
					placeholder={tab === "agents" ? "Buscar agentes…" : "Buscar workspaces…"}
				/>

				{activeWorkspace ? (
					<button
						type="button"
						onClick={() => setWorkspaceFilter(null)}
						className="mb-3 inline-flex items-center gap-1.5 rounded-full border border-accent/40 bg-accent/10 px-2.5 py-1 font-mono text-[11px] text-accent"
					>
						<FolderGit2 className="size-3" />
						{activeWorkspace.name ?? activeWorkspace.projectName ?? activeWorkspace.id}
						<X className="size-3" />
					</button>
				) : null}

				{tab === "workspaces" ? (
					<WorkspacesList
						combs={filteredCombs}
						sessionCount={sessionsByWorkspace}
						diffs={diffs}
						onPick={(id) => {
							setWorkspaceFilter(id);
							setTab("agents");
							setSearch("");
						}}
					/>
				) : flatSearch !== null ? (
					<FlatResults sessions={flatSearch} running={scan?.running ?? new Set()} />
				) : (
					<Triage
						groups={groups}
						resolving={resolving}
						onRespond={(item, choice) => void respond(item, choice)}
					/>
				)}
			</div>
		</Shell>
	);
}

function Segmented({ tab, onChange }: { tab: Tab; onChange: (t: Tab) => void }) {
	const tabs: Array<[Tab, string]> = [
		["agents", "Agentes"],
		["workspaces", "Workspaces"],
	];
	return (
		<div className="mb-3 flex gap-1 rounded-xl border border-border bg-panel p-1">
			{tabs.map(([id, label]) => (
				<button
					key={id}
					type="button"
					onClick={() => onChange(id)}
					className={cn(
						"flex-1 rounded-lg py-1.5 font-mono text-[11px] uppercase tracking-wider transition-colors",
						tab === id
							? "bg-elevated text-foreground"
							: "text-mute hover:text-foreground",
					)}
				>
					{label}
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
		<div className="mb-4 flex items-center gap-2 rounded-xl border border-border bg-panel px-3 py-2 focus-within:border-accent/40">
			<Search className="size-3.5 shrink-0 text-faint" />
			<input
				type="text"
				value={value}
				onChange={(e) => onChange(e.target.value)}
				placeholder={placeholder}
				className="flex-1 bg-transparent text-[13px] outline-none placeholder:text-faint"
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

function DaemonBar({
	status,
	refreshing,
	onRefresh,
}: {
	status: DaemonStatus | null;
	refreshing: boolean;
	onRefresh: () => void;
}) {
	const live = status?.running ?? false;
	return (
		<div className="flex items-center justify-between gap-3 rounded-xl border border-border bg-panel px-3.5 py-2.5">
			<div className="flex items-center gap-2.5">
				<StateDot state={live ? "live" : "idle"} />
				<span className="text-[13px] font-medium">
					{status ? (live ? "Daemon ativo" : "Daemon parado") : "Conectando…"}
				</span>
				{status ? (
					<span className="font-mono text-[11px] text-faint">
						<Cpu className="mr-1 inline size-3" />
						{status.cpuPercent.toFixed(0)}% · {status.memoryMb.toFixed(0)}MB
					</span>
				) : null}
			</div>
			<button
				type="button"
				onClick={onRefresh}
				disabled={refreshing}
				className="rounded-lg p-1 text-mute hover:text-foreground disabled:opacity-50"
				title="Atualizar"
			>
				<RefreshCw className={cn("size-3.5", refreshing && "animate-spin")} />
			</button>
		</div>
	);
}

function Triage({
	groups,
	resolving,
	onRespond,
}: {
	groups: { needsYou: PendingItem[]; running: SessionSearchResult[]; recent: SessionSearchResult[] } | null;
	resolving: Set<string>;
	onRespond: (item: PendingItem, choice: string) => void;
}) {
	if (!groups) {
		return (
			<div className="flex justify-center py-10 text-mute">
				<Loader2 className="size-5 animate-spin" />
			</div>
		);
	}

	const { needsYou, running, recent } = groups;

	if (needsYou.length === 0 && running.length === 0 && recent.length === 0) {
		return (
			<Rest icon={<Inbox className="size-6" strokeWidth={1.6} />} title="Nada por aqui">
				Crie uma thread no botão + para colocar um agente pra trabalhar.
			</Rest>
		);
	}

	return (
		<div className="space-y-6">
			{needsYou.length > 0 ? (
				<section>
					<SectionLabel tone="wait" count={needsYou.length}>
						Precisa de você
					</SectionLabel>
					<ul className="space-y-2.5">
						{needsYou.map((item) => {
							const key = `${item.thread.sessionId}/${item.requestId}`;
							return (
								<li key={key}>
									<NeedsYouCard
										item={item}
										resolving={resolving.has(key)}
										onRespond={(choice) => onRespond(item, choice)}
									/>
								</li>
							);
						})}
					</ul>
				</section>
			) : null}

			{running.length > 0 ? (
				<section>
					<SectionLabel tone="live" count={running.length}>
						Rodando
					</SectionLabel>
					<ul className="space-y-2">
						{running.map((s) => (
							<li key={s.sessionId}>
								<SessionRow session={s} state="live" />
							</li>
						))}
					</ul>
				</section>
			) : null}

			{recent.length > 0 ? (
				<section>
					<SectionLabel>Recentes</SectionLabel>
					<ul className="space-y-2">
						{recent.map((s) => (
							<li key={s.sessionId}>
								<SessionRow session={s} state="idle" />
							</li>
						))}
					</ul>
				</section>
			) : null}
		</div>
	);
}

function FlatResults({
	sessions,
	running,
}: {
	sessions: SessionSearchResult[];
	running: Set<string>;
}) {
	if (sessions.length === 0) {
		return <Rest title="Nenhum resultado">Tente outro termo.</Rest>;
	}
	return (
		<ul className="space-y-2">
			{sessions.map((s) => (
				<li key={s.sessionId}>
					<SessionRow
						session={s}
						state={running.has(s.sessionId) ? "live" : "idle"}
					/>
				</li>
			))}
		</ul>
	);
}

function NeedsYouCard({
	item,
	resolving,
	onRespond,
}: {
	item: PendingItem;
	resolving: boolean;
	onRespond: (choice: string) => void;
}) {
	const title = sessionTitle(item.thread);
	const place = workspaceLabel(item.thread);
	return (
		<div className="overflow-hidden rounded-2xl border border-wait/40 bg-wait/[0.06]">
			<Link
				to="/threads/$threadId"
				params={{ threadId: item.thread.sessionId }}
				className="flex items-center gap-2.5 px-3.5 pt-3 active:opacity-70"
			>
				<StateDot state="wait" />
				<div className="min-w-0 flex-1">
					<p className="truncate text-[13px] font-medium">{title}</p>
					{place ? (
						<p className="truncate font-mono text-[10px] text-mute">{place}</p>
					) : null}
				</div>
				<ChevronRight className="size-3.5 text-faint" />
			</Link>
			<p className="px-3.5 py-2.5 text-[13px] leading-snug text-foreground/90">
				{item.question}
			</p>
			<div className="flex gap-2 px-3.5 pb-3">
				{item.choices.map((choice) => {
					const deny = choice.id === "deny";
					return (
						<button
							key={choice.id}
							type="button"
							disabled={resolving}
							onClick={() => onRespond(choice.id)}
							className={cn(
								"flex-1 rounded-lg px-3 py-2 text-[13px] font-semibold transition-opacity disabled:opacity-50",
								deny
									? "border border-border bg-bg text-foreground active:bg-elevated"
									: "bg-wait text-[var(--color-wait-ink)] active:opacity-80",
							)}
						>
							{resolving && !deny ? (
								<Loader2 className="mx-auto size-4 animate-spin" />
							) : (
								choice.label
							)}
						</button>
					);
				})}
			</div>
		</div>
	);
}

function SessionRow({
	session,
	state,
}: {
	session: SessionSearchResult;
	state: AgentState;
}) {
	const title = sessionTitle(session);
	const place = workspaceLabel(session);
	return (
		<Link
			to="/threads/$threadId"
			params={{ threadId: session.sessionId }}
			className="flex items-center gap-3 rounded-xl border border-border bg-panel px-3.5 py-3 active:bg-elevated"
		>
			<StateDot state={state} className="mt-0.5 self-start" />
			<div className="min-w-0 flex-1">
				<div className="flex items-center gap-2">
					<p className="truncate text-[13.5px] font-medium">{title}</p>
					<ProviderTag providerId={session.providerId} />
				</div>
				{place ? (
					<p className="mt-0.5 flex items-center gap-1 truncate font-mono text-[10px] text-mute">
						{session.workspaceBranch ? (
							<GitBranch className="size-3 shrink-0 text-faint" />
						) : null}
						<span className="truncate">{place}</span>
					</p>
				) : null}
				{state === "live" ? (
					<p className="mt-1 font-mono text-[10px] uppercase tracking-wider text-accent">
						trabalhando…
					</p>
				) : (
					<p className="mt-1 font-mono text-[10px] uppercase tracking-wider text-faint">
						{formatRelative(session.updatedAt)}
					</p>
				)}
			</div>
			<ChevronRight className="size-4 shrink-0 self-center text-faint" />
		</Link>
	);
}

function WorkspacesList({
	combs,
	sessionCount,
	diffs,
	onPick,
}: {
	combs: Comb[] | null;
	sessionCount: Map<string, number>;
	diffs: Map<string, WorktreeDiff> | null;
	onPick: (id: string) => void;
}) {
	if (combs === null) {
		return (
			<div className="flex justify-center py-10 text-mute">
				<Loader2 className="size-5 animate-spin" />
			</div>
		);
	}
	if (combs.length === 0) {
		return <Rest title="Nenhum workspace">Crie um pelo app desktop.</Rest>;
	}
	return (
		<ul className="space-y-2">
			{combs.map((c) => {
				const count = sessionCount.get(c.id) ?? 0;
				const diff = c.worktreePath ? diffs?.get(c.worktreePath) ?? null : null;
				return (
					<li
						key={c.id}
						className="flex items-stretch overflow-hidden rounded-xl border border-border bg-panel"
					>
						<button
							type="button"
							onClick={() => onPick(c.id)}
							className="flex min-w-0 flex-1 items-center gap-3 px-3.5 py-3 text-left active:bg-elevated"
						>
							<FolderGit2 className="size-4 shrink-0 text-mute" />
							<div className="min-w-0 flex-1">
								<p className="truncate text-[13.5px] font-medium">
									{c.name ?? c.projectName ?? c.id}
								</p>
								<p className="mt-0.5 flex items-center gap-1 truncate font-mono text-[10px] text-mute">
									{c.branch ? <GitBranch className="size-3 text-faint" /> : null}
									<span className="truncate">
										{[c.projectName, c.branch].filter(Boolean).join(" · ")}
									</span>
									{count > 0 ? (
										<span className="text-faint">· {count} sess</span>
									) : null}
								</p>
							</div>
						</button>
						<Link
							to="/diff/$combId"
							params={{ combId: c.id }}
							title="Ver o que mudou"
							className="flex shrink-0 items-center gap-1.5 border-l border-border px-3 font-mono text-[11px] tabular-nums active:bg-elevated"
						>
							<DiffPill diff={diffs ? diff : undefined} />
						</Link>
					</li>
				);
			})}
		</ul>
	);
}

/** Trailing +/- pill: undefined = still loading, null = clean / unknown. */
function DiffPill({ diff }: { diff: WorktreeDiff | null | undefined }) {
	if (diff === undefined) {
		return <Loader2 className="size-3 animate-spin text-faint" />;
	}
	if (!diff || diff.clean) {
		return <span className="text-faint">limpo</span>;
	}
	return (
		<span className="flex items-center gap-1.5">
			<span className="text-accent">+{diff.insertions}</span>
			<span className="text-danger">−{diff.deletions}</span>
			<ChevronRight className="size-3.5 text-faint" />
		</span>
	);
}

function ProviderTag({ providerId }: { providerId: string | null }) {
	if (!providerId) return null;
	const label =
		providerId === "claude_code"
			? "claude"
			: providerId === "codex"
				? "codex"
				: providerId;
	return (
		<span className="shrink-0 rounded border border-border bg-bg px-1 py-px font-mono text-[9px] lowercase tracking-wide text-faint">
			{label}
		</span>
	);
}

function sessionTitle(s: SessionSearchResult): string {
	return (
		s.threadTitle?.trim() ||
		s.workspaceName ||
		s.projectId ||
		"Sessão sem título"
	);
}

function workspaceLabel(s: SessionSearchResult): string {
	return [s.workspaceName ?? s.projectId, s.workspaceBranch]
		.filter(Boolean)
		.join(" · ");
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

/**
 * One sweep that powers all three triage groups. We fetch the session
 * index, then fan out events only for threads touched in the last 48h
 * (capped) — the same budget the old permission counter spent, now reused
 * to also detect which agents are mid-turn.
 */
async function runScan(
	session: PairingSession,
): Promise<{ scan: Scan; combs: Comb[] }> {
	const [sessRaw, combs] = await Promise.all([
		apiFetch<SessionSearchResult[]>(session, "/api/v1/sessions/search?limit=60"),
		apiFetch<Comb[]>(session, "/api/v1/combs").catch(() => [] as Comb[]),
	]);
	const sessions = sessRaw.filter((s) => !s.archivedAt);

	const cutoff = Date.now() - RECENT_SCAN_WINDOW_MS;
	const recent = sessions
		.filter((t) => {
			const ts = Date.parse(t.updatedAt);
			return Number.isFinite(ts) && ts > cutoff;
		})
		.slice(0, RECENT_SCAN_LIMIT);

	const sweeps = await Promise.all(
		recent.map((thread) =>
			apiFetch<RawSessionEvent[]>(
				session,
				`/api/v1/sessions/${encodeURIComponent(thread.sessionId)}/events`,
			)
				.then((events) => ({ thread, events }))
				.catch(() => ({ thread, events: [] as RawSessionEvent[] })),
		),
	);

	const running = new Set<string>();
	const needsYou: PendingItem[] = [];
	for (const { thread, events } of sweeps) {
		const requested = new Map<string, RawSessionEvent>();
		const resolved = new Set<string>();
		let live = false;
		for (const event of events) {
			const kind = event.kind;
			switch (kind?.type) {
				case "turn_started":
					live = true;
					break;
				case "turn_completed":
				case "turn_aborted":
				case "session_completed":
				case "session_aborted":
					live = false;
					break;
				case "turn_permission_requested": {
					const id =
						typeof kind.requestId === "string" ? kind.requestId : event.eventId;
					requested.set(id, event);
					break;
				}
				case "turn_permission_resolved": {
					const id = typeof kind.requestId === "string" ? kind.requestId : "";
					resolved.add(id);
					break;
				}
			}
		}
		if (live) running.add(thread.sessionId);
		for (const [id, event] of requested) {
			if (resolved.has(id)) continue;
			const kind = event.kind ?? {};
			const choices = Array.isArray(kind.choices)
				? (kind.choices as Array<{ id: string; label: string }>)
				: [
						{ id: "allow", label: "Permitir" },
						{ id: "deny", label: "Negar" },
					];
			needsYou.push({
				thread,
				requestId: id,
				question:
					typeof kind.question === "string" ? kind.question : "Pedido de permissão",
				choices,
				at: event.occurredAt,
			});
		}
	}
	needsYou.sort((a, b) => b.at.localeCompare(a.at));

	return { scan: { sessions, running, needsYou }, combs };
}
