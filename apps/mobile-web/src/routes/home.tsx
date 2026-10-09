import { ProjectGlyph } from "../components/project-glyph";
import { useRef, useEffect, useMemo, useState } from "react";
import { Link } from "@tanstack/react-router";
import {
	ChevronDown,
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
import { readLocal, writeLocal } from "@/lib/local-data";
import { ConnectionNotice } from "@/components/connection-notice";
import { openEventStream, type StreamState } from "@/lib/sseClient";
import {
	combsByProject,
	formatRelative,
	incomingEventKindType,
	latestSessionByWorkspace,
	normalizeProvider,
	runScan as scanSessions,
	searchSessions,
	sessionTitle,
	sortCombs,
	triageGroups,
	withoutPending,
	workspaceLabel,
	type Comb,
	type LiveStatus,
	type PendingItem,
	type Scan,
	type SessionSearchResult,
} from "@dcc/mobile-core";
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

type Tab = "agents" | "workspaces";

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
	const [link, setLink] = useState("");
	const [error, setError] = useState<string | null>(null);
	const connect = () => {
		try {
			const url = new URL(link.trim());
			if (
				url.origin !== location.origin ||
				url.pathname !== "/m/pair" ||
				!new URLSearchParams(url.hash.slice(1)).get("nonce")
			)
				throw new Error(
					"Use o link de pareamento deste mesmo endereço do DCC.",
				);
			location.assign(url.href);
		} catch (e) {
			setError(e instanceof Error ? e.message : "Link inválido.");
		}
	};
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
				<label className="mt-5 block text-xs text-mute">
					Já abriu pelo ícone instalado? Cole o link de pareamento:
					<input
						type="url"
						value={link}
						onChange={(e) => setLink(e.target.value)}
						placeholder="https://…/m/pair#nonce=…"
						className="mt-2 w-full rounded-xl border border-border bg-bg p-3 text-base"
					/>
				</label>
				<button
					type="button"
					disabled={!link.trim()}
					onClick={connect}
					className="mt-3 w-full rounded-xl bg-accent p-3 font-semibold text-accent-ink disabled:opacity-40"
				>
					Continuar pareamento
				</button>
				{error && (
					<p role="alert" className="mt-2 text-xs text-danger">
						{error}
					</p>
				)}
			</section>
		</Shell>
	);
}

function PairedHome({ session }: { session: PairingSession }) {
	const refreshLock = useRef(false);
	const [connection, setConnection] = useState<StreamState>("connecting");
	const [lastSynced, setLastSynced] = useState<string | null>(null);
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
		if (refreshLock.current) return;
		refreshLock.current = true;
		setRefreshing(true);
		setError(null);
		const statusController = new AbortController();
		const statusTimeout = setTimeout(() => statusController.abort(), 5000);
		void apiFetch<DaemonStatus>(session, "/api/v1/status", {
			signal: statusController.signal,
		})
			.then(setStatus)
			.catch(() => setStatus(null))
			.finally(() => clearTimeout(statusTimeout));
		try {
			const result = await runScan(session);
			setScan(result.scan);
			setCombs(result.combs);
			const at = new Date().toISOString();
			setLastSynced(at);
			writeLocal(session, "home", {
				at,
				combs: result.combs,
				sessions: result.scan.sessions,
				needsYou: result.scan.needsYou,
				statuses: [...result.scan.statuses],
			});
		} catch (err) {
			if (err instanceof ApiError && err.status === 401) {
				setError("Sessão expirada. Pareie de novo no desktop.");
			} else {
				setError(err instanceof Error ? err.message : "Falha ao carregar.");
			}
		} finally {
			setRefreshing(false);
			refreshLock.current = false;
		}
	};

	useEffect(() => {
		const cached = readLocal<{
			at: string;
			combs: Comb[];
			sessions: SessionSearchResult[];
			needsYou: PendingItem[];
			statuses: [string, LiveStatus][];
		}>(session, "home");
		if (cached) {
			setCombs(cached.combs);
			setLastSynced(cached.at);
			setScan({
				sessions: cached.sessions,
				needsYou: cached.needsYou,
				statuses: new Map(cached.statuses),
			});
		}
		void refresh();
		const visible = () => {
			if (document.visibilityState === "visible") void refresh();
		};
		const id = window.setInterval(visible, 15_000);
		document.addEventListener("visibilitychange", visible);
		window.addEventListener("online", visible);
		return () => {
			window.clearInterval(id);
			document.removeEventListener("visibilitychange", visible);
			window.removeEventListener("online", visible);
		};
	}, []);

	useEffect(() => {
		const stop = openEventStream(session, "/api/v1/events/stream", {
			onOpen: () => void refresh(),
			onState: setConnection,
			onMessage: (payload) => {
				const kind = incomingEventKindType(payload);
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
						behavior: choice,
					}),
				},
			);
			setScan((prev) =>
				prev ? withoutPending(prev, item.thread.sessionId, item.requestId) : prev,
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

	const flatSearch = useMemo(
		() => (sessions ? searchSessions(sessions, search, workspaceFilter) : null),
		[sessions, search, workspaceFilter],
	);

	const groups = useMemo(
		() => (scan ? triageGroups(scan, workspaceFilter) : null),
		[scan, workspaceFilter],
	);

	const filteredCombs = useMemo(
		() => (combs ? sortCombs(combs, sessions ?? [], search) : null),
		[combs, search, sessions],
	);

	const activeWorkspace = workspaceFilter
		? (combs?.find((c) => c.id === workspaceFilter) ?? null)
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
							needsYouCount > 0
								? "text-wait"
								: "text-mute hover:text-foreground",
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
						title="Nova tarefa"
						aria-label="Nova tarefa"
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

			<Link
				to="/new"
				className="mb-4 flex min-h-12 items-center justify-center gap-2 rounded-2xl bg-accent px-4 py-3 font-semibold text-accent-ink"
			>
				<Plus className="size-5" />
				Nova tarefa
			</Link>

			<ConnectionNotice
				state={connection}
				lastSynced={lastSynced}
				onRetry={() => void refresh()}
			/>
			<DaemonBar
				status={status}
				refreshing={refreshing}
				onRefresh={() => void refresh()}
			/>

			{error ? (
				<p className="mt-3 rounded-xl border border-danger/30 bg-danger/5 px-4 py-3 text-[12px] text-danger">
					{error}
				</p>
			) : null}

			<div className="mt-5">
				<Segmented
					tab={tab}
					onChange={(t) => {
						setTab(t);
						setWorkspaceFilter(null);
						setSearch("");
					}}
				/>

				<SearchBar
					value={search}
					onChange={setSearch}
					placeholder={
						tab === "agents" ? "Buscar agentes…" : "Buscar workspaces…"
					}
				/>

				{activeWorkspace ? (
					<button
						type="button"
						onClick={() => setWorkspaceFilter(null)}
						className="mb-3 inline-flex items-center gap-1.5 rounded-full border border-accent/40 bg-accent/10 px-2.5 py-1 font-mono text-[11px] text-accent"
					>
						<FolderGit2 className="size-3" />
						{activeWorkspace.name ??
							activeWorkspace.projectName ??
							activeWorkspace.id}
						<X className="size-3" />
					</button>
				) : null}

				{tab === "workspaces" ? (
					<WorkspacesList
						combs={filteredCombs}
						sessions={sessions ?? []}
						statuses={scan?.statuses ?? new Map()}
						diffs={diffs}
					/>
				) : flatSearch !== null ? (
					<FlatResults
						sessions={flatSearch}
						statuses={scan?.statuses ?? new Map()}
					/>
				) : (
					<Triage
						groups={groups}
						statuses={scan?.statuses ?? new Map()}
						resolving={resolving}
						onRespond={(item, choice) => void respond(item, choice)}
					/>
				)}
			</div>
		</Shell>
	);
}

function Segmented({
	tab,
	onChange,
}: {
	tab: Tab;
	onChange: (t: Tab) => void;
}) {
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
	statuses,
	resolving,
	onRespond,
}: {
	groups: {
		needsYou: PendingItem[];
		running: SessionSearchResult[];
		recent: SessionSearchResult[];
	} | null;
	statuses: Map<string, LiveStatus>;
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
			<Rest
				icon={<Inbox className="size-6" strokeWidth={1.6} />}
				title="Nada por aqui"
			>
				Toque em Nova tarefa para colocar um agente para trabalhar.
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
								<SessionRow session={s} status="running" />
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
								<SessionRow session={s} status={statuses.get(s.sessionId)} />
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
	statuses,
}: {
	sessions: SessionSearchResult[];
	statuses: Map<string, LiveStatus>;
}) {
	if (sessions.length === 0) {
		return <Rest title="Nenhum resultado">Tente outro termo.</Rest>;
	}
	return (
		<ul className="space-y-2">
			{sessions.map((s) => (
				<li key={s.sessionId}>
					<SessionRow session={s} status={statuses.get(s.sessionId)} />
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
				{item.choices.length === 0 && (
					<Link
						to="/threads/$threadId"
						params={{ threadId: item.thread.sessionId }}
						className="min-h-11 w-full rounded-xl bg-wait p-3 text-center text-sm font-semibold text-wait-ink"
					>
						Responder ao agente
					</Link>
				)}
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

const DOT_FOR_STATUS: Record<LiveStatus, AgentState> = {
	running: "live",
	completed: "idle",
	aborted: "fail",
};

function SessionRow({
	session,
	status,
}: {
	session: SessionSearchResult;
	status?: LiveStatus;
}) {
	const title = sessionTitle(session);
	const workspace = session.workspaceName ?? session.projectId;
	return (
		<Link
			to="/threads/$threadId"
			params={{ threadId: session.sessionId }}
			className="flex items-center gap-3 rounded-xl border border-border bg-panel px-3.5 py-3 active:bg-elevated"
		>
			<StateDot
				state={status ? DOT_FOR_STATUS[status] : "idle"}
				className="mt-0.5 self-start"
			/>
			<div className="min-w-0 flex-1">
				<div className="flex items-center gap-2">
					<p className="truncate text-[13.5px] font-medium">{title}</p>
					<ProviderTag providerId={session.providerId} />
				</div>
				{workspace ? (
					<p className="mt-0.5 flex items-center gap-1 truncate font-mono text-[10px] text-mute">
						<FolderGit2 className="size-3 shrink-0 text-faint" />
						<span className="truncate text-foreground/70">{workspace}</span>
						{session.workspaceBranch ? (
							<>
								<GitBranch className="size-3 shrink-0 text-faint" />
								<span className="truncate">{session.workspaceBranch}</span>
							</>
						) : null}
					</p>
				) : null}
				<div className="mt-1 flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-wider">
					<StatusText status={status} />
					<span className="text-faint">
						· {formatRelative(session.updatedAt)}
					</span>
				</div>
			</div>
			<ChevronRight className="size-4 shrink-0 self-center text-faint" />
		</Link>
	);
}

function StatusText({ status }: { status?: LiveStatus }) {
	if (status === "running")
		return <span className="text-accent">trabalhando…</span>;
	if (status === "completed")
		return <span className="text-accent/80">Concluído</span>;
	if (status === "aborted")
		return <span className="text-danger">Interrompido</span>;
	return <span className="text-faint">aberta</span>;
}

function WorkspacesList({
	combs,
	sessions,
	statuses,
	diffs,
}: {
	combs: Comb[] | null;
	sessions: SessionSearchResult[];
	statuses: Map<string, LiveStatus>;
	diffs: Map<string, WorktreeDiff> | null;
}) {
	const latestByWorkspace = useMemo(
		() => latestSessionByWorkspace(sessions),
		[sessions],
	);
	const projects = useMemo(() => combsByProject(combs ?? []), [combs]);

	const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());

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
	const toggle = (name: string) =>
		setCollapsed((prev) => {
			const next = new Set(prev);
			if (next.has(name)) next.delete(name);
			else next.add(name);
			return next;
		});
	return (
		<div className="space-y-4">
			{projects.map(([projectName, projectCombs]) => {
				const isCollapsed = collapsed.has(projectName);
				return (
					<section key={projectName}>
						<button
							type="button"
							onClick={() => toggle(projectName)}
							className="mb-2 flex w-full items-center gap-2 px-0.5"
						>
							<ChevronDown
								className={cn(
									"size-3.5 shrink-0 text-faint transition-transform",
									isCollapsed && "-rotate-90",
								)}
							/>
							<ProjectGlyph
								icon={projectCombs[0]?.projectIcon}
								color={projectCombs[0]?.projectColor}
								seed={projectCombs[0]?.projectPath}
								active={projectCombs.some((comb) => {
									const sess = latestByWorkspace.get(comb.id);
									return sess ? statuses.get(sess.sessionId) === "running" : false;
								})}
							/>
							<span className="truncate text-[12px] font-semibold">
								{projectName}
							</span>
							<span className="shrink-0 rounded-full bg-elevated px-1.5 font-mono text-[10px] tabular-nums text-mute">
								{projectCombs.length}
							</span>
							<span className="h-px flex-1 bg-border/60" />
						</button>
						{isCollapsed ? null : (
							<ul className="space-y-2">
								{projectCombs.map((c) => {
									const sess = latestByWorkspace.get(c.id) ?? null;
									const status = sess
										? statuses.get(sess.sessionId)
										: undefined;
									const diff = c.worktreePath
										? (diffs?.get(c.worktreePath) ?? null)
										: null;
									const body = (
										<div className="flex min-w-0 flex-1 items-center gap-3 px-3.5 py-3">
											<StateDot
												state={status ? DOT_FOR_STATUS[status] : "idle"}
											/>
											<div className="min-w-0 flex-1">
												<p className="truncate text-[13.5px] font-medium">
													{c.name}
												</p>
												<p className="mt-0.5 flex items-center gap-1.5 truncate font-mono text-[10px] uppercase tracking-wider">
													{c.branch ? (
														<span className="flex items-center gap-1 text-mute">
															<GitBranch className="size-3 shrink-0 text-faint" />
															{c.branch}
														</span>
													) : null}
													<StatusText status={status} />
												</p>
											</div>
										</div>
									);
									return (
										<li
											key={c.id}
											className="flex items-stretch overflow-hidden rounded-xl border border-border bg-panel"
										>
											{sess ? (
												<Link
													to="/threads/$threadId"
													params={{ threadId: sess.sessionId }}
													className="flex min-w-0 flex-1 active:bg-elevated"
												>
													{body}
												</Link>
											) : (
												<div className="flex min-w-0 flex-1 opacity-60">
													{body}
												</div>
											)}
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
						)}
					</section>
				);
			})}
		</div>
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
	return (
		<span className="shrink-0 rounded border border-border bg-bg px-1 py-px font-mono text-[9px] lowercase tracking-wide text-faint">
			{normalizeProvider(providerId)}
		</span>
	);
}

function runScan(session: PairingSession): Promise<{ scan: Scan; combs: Comb[] }> {
	return scanSessions((path) => apiFetch(session, path));
}
