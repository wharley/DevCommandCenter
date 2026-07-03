import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import {
	AlertTriangle,
	ArrowLeft,
	Brain,
	Check,
	ChevronRight,
	GitBranch,
	Loader2,
	Send,
	ShieldCheck,
	StopCircle,
	Wrench,
	X,
} from "lucide-react";
import { Markdown } from "@/components/Markdown";
import { Rest, Shell, StateDot } from "@/components/ui";
import { apiFetch } from "@/lib/api";
import { foldEntry, type BundleEntry, type WorktreeDiff } from "@/lib/diff";
import { cn } from "@/lib/cn";
import { loadSession, type PairingSession } from "@/lib/session";
import { openEventStream } from "@/lib/sseClient";
import {
	applyEvents,
	applyIncomingEvent,
	createThreadState,
	incomingEventSessionId,
	type ChatMessage,
	type RawSessionEvent,
	type ThreadState,
} from "@/lib/threadEvents";

type SessionMeta = {
	sessionId: string;
	workspaceId: string | null;
	workspaceName: string | null;
	workspaceBranch: string | null;
};

type SendTurnOutput = {
	turn?: {
		id?: string;
		content?: string;
		createdAt?: string;
	};
};

export function ThreadRoute() {
	const params = useParams({ from: "/threads/$threadId" });
	const threadId = params.threadId;
	const navigate = useNavigate();
	const [session, setSession] = useState<PairingSession | null | undefined>(undefined);
	const [state, setState] = useState<ThreadState>(() => createThreadState());
	const [loading, setLoading] = useState(true);
	const [loadError, setLoadError] = useState<string | null>(null);
	const [sending, setSending] = useState(false);
	const [composer, setComposer] = useState("");
	const [aborting, setAborting] = useState(false);
	const [meta, setMeta] = useState<SessionMeta | null>(null);
	const [diff, setDiff] = useState<WorktreeDiff | null>(null);
	const scrollerRef = useRef<HTMLDivElement | null>(null);

	useEffect(() => {
		void loadSession().then((s) => setSession(s));
	}, []);

	useEffect(() => {
		if (!session) return;
		let cancelled = false;
		setLoading(true);
		setLoadError(null);
		(async () => {
			try {
				const events = await apiFetch<RawSessionEvent[]>(
					session,
					`/api/v1/sessions/${encodeURIComponent(threadId)}/events`,
				);
				if (cancelled) return;
				setState((prev) => applyEvents(prev, events));
				setLoading(false);
			} catch (err) {
				if (cancelled) return;
				setLoadError(err instanceof Error ? err.message : "Falha ao carregar.");
				setLoading(false);
			}
		})();
		return () => {
			cancelled = true;
		};
	}, [session, threadId]);

	// Listen for new events via SSE.
	useEffect(() => {
		if (!session) return;
		const stop = openEventStream(session, "/api/v1/events/stream", {
			onMessage: (payload) => {
				if (incomingEventSessionId(payload) !== threadId) return;
				setState((prev) => applyIncomingEvent(prev, payload));
			},
			onError: () => {
				/* reconnect handled inside openEventStream */
			},
		});
		return stop;
	}, [session, threadId]);

	// Auto-scroll to bottom when new messages arrive.
	useEffect(() => {
		const el = scrollerRef.current;
		if (!el) return;
		el.scrollTop = el.scrollHeight;
	}, [state.cursor, state.messages.length]);

	const lastTurnId = useMemo(() => {
		for (let i = state.messages.length - 1; i >= 0; i--) {
			const m = state.messages[i];
			if (m && "turnId" in m && m.turnId) return m.turnId;
		}
		return null;
	}, [state.messages]);

	// Running = anything in flight: a streaming answer, a live tool call or
	// reasoning block, or a prompt that hasn't produced output yet. Walking
	// backwards means the freshest signal wins.
	const isRunning = useMemo(() => {
		for (let i = state.messages.length - 1; i >= 0; i--) {
			const m = state.messages[i];
			if (!m) continue;
			if (m.kind === "assistant") return !m.completed && !m.aborted;
			if (m.kind === "tool") {
				if (m.status === "running") return true;
				continue;
			}
			if (m.kind === "reasoning") {
				if (!m.completed) return true;
				continue;
			}
			if (m.kind === "user") return true;
			if (m.kind === "system") return false;
		}
		return false;
	}, [state.messages]);

	const threadTitle = useMemo(() => {
		const firstUser = state.messages.find(
			(m) => m.kind === "user" && m.text.trim().length > 0,
		);
		if (firstUser && firstUser.kind === "user") {
			const t = firstUser.text.trim().replace(/\s+/g, " ");
			return t.length > 52 ? `${t.slice(0, 51)}…` : t;
		}
		return "Nova sessão";
	}, [state.messages]);

	// There's no per-session metadata endpoint, so locate this session in the
	// search index once to learn which workspace (comb) it belongs to. Drives
	// the workspace · branch subtitle and the live diff affordance.
	useEffect(() => {
		if (!session) return;
		let cancelled = false;
		apiFetch<SessionMeta[]>(session, "/api/v1/sessions/search?limit=120")
			.then((rows) => {
				if (cancelled) return;
				const row = rows.find((r) => r.sessionId === threadId);
				if (row) setMeta(row);
			})
			.catch(() => {
				/* non-fatal: the header just stays minimal */
			});
		return () => {
			cancelled = true;
		};
	}, [session, threadId]);

	// Pull the worktree diff for this session's comb, refreshing whenever a
	// turn starts or finishes — so "+142 −7" tracks the agent's edits live.
	const combId = meta?.workspaceId ?? null;
	useEffect(() => {
		if (!session || !combId) return;
		let cancelled = false;
		apiFetch<BundleEntry[]>(session, "/api/v1/diffs/bundle", {
			method: "POST",
			body: JSON.stringify({ combIds: [combId], worktreePaths: [] }),
		})
			.then((bundle) => {
				if (!cancelled && bundle[0]) setDiff(foldEntry(bundle[0]));
			})
			.catch(() => {
				/* non-fatal */
			});
		return () => {
			cancelled = true;
		};
	}, [session, combId, isRunning]);

	const submit = async () => {
		if (!session) return;
		const text = composer.trim();
		if (!text || sending) return;
		setSending(true);
		try {
			const result = await apiFetch<SendTurnOutput>(
				session,
				`/api/v1/sessions/${encodeURIComponent(threadId)}/turns`,
				{
					method: "POST",
					body: JSON.stringify({ sessionId: threadId, prompt: text }),
				},
			);
			const turnId = result.turn?.id;
			if (turnId) {
				setState((prev) =>
					applyEvents(prev, [
						{
							eventId: `local:${turnId}`,
							sessionId: threadId,
							sequence: prev.cursor + 1,
							occurredAt: result.turn?.createdAt ?? new Date().toISOString(),
							kind: {
								type: "turn_started",
								turnId,
								prompt: result.turn?.content ?? text,
							},
						},
					]),
				);
			}
			setComposer("");
		} catch (err) {
			setLoadError(err instanceof Error ? err.message : "Falha ao enviar.");
		} finally {
			setSending(false);
		}
	};

	const abort = async () => {
		if (!session || aborting) return;
		setAborting(true);
		try {
			await apiFetch(session, `/api/v1/sessions/${encodeURIComponent(threadId)}/abort`, {
				method: "POST",
				body: JSON.stringify({ sessionId: threadId }),
			});
		} catch (err) {
			setLoadError(err instanceof Error ? err.message : "Falha ao abortar.");
		} finally {
			setAborting(false);
		}
	};

	const respondPermission = async (requestId: string, choice: string) => {
		if (!session) return;
		try {
			await apiFetch(
				session,
				`/api/v1/sessions/${encodeURIComponent(threadId)}/respond-permission`,
				{
					method: "POST",
					body: JSON.stringify({
						sessionId: threadId,
						requestId,
						behavior: choice,
					}),
				},
			);
		} catch (err) {
			setLoadError(err instanceof Error ? err.message : "Falha ao responder.");
		}
	};

	if (session === undefined) {
		return (
			<Shell>
				<div className="flex h-[60vh] items-center justify-center text-mute">
					<Loader2 className="size-5 animate-spin" />
				</div>
			</Shell>
		);
	}

	if (session === null) {
		return (
			<Shell>
				<header className="px-1 pb-5">
					<h1 className="text-xl font-semibold">Sem sessão</h1>
				</header>
				<p className="rounded-2xl border border-border bg-panel p-4 text-[13px] text-mute">
					Você precisa parear o celular antes de abrir uma sessão.{" "}
					<Link to="/" className="text-foreground underline">
						Voltar
					</Link>
					.
				</p>
			</Shell>
		);
	}

	return (
		<div className="flex h-dvh flex-col">
			<TopBar
				title={threadTitle}
				subtitle={workspaceLabel(meta)}
				isRunning={isRunning}
				combId={combId}
				diff={diff}
				onBack={() => void navigate({ to: "/" })}
			/>

			<div ref={scrollerRef} className="flex-1 overflow-y-auto">
				<div className="mx-auto max-w-md px-4 py-5">
					{loading ? (
						<div className="flex h-[40vh] items-center justify-center text-mute">
							<Loader2 className="size-5 animate-spin" />
						</div>
					) : loadError ? (
						<ErrorBanner message={loadError} onDismiss={() => setLoadError(null)} />
					) : state.messages.length === 0 ? (
						<Rest title="Sem mensagens ainda">
							Descreva a primeira tarefa no campo abaixo para colocar o agente
							pra trabalhar.
						</Rest>
					) : (
						<MessageList
							messages={state.messages}
							onRespondPermission={(reqId, choice) => void respondPermission(reqId, choice)}
						/>
					)}
				</div>
			</div>

			<Composer
				value={composer}
				onChange={setComposer}
				onSubmit={() => void submit()}
				sending={sending}
				running={isRunning}
				aborting={aborting}
				onAbort={() => void abort()}
				placeholder={
					lastTurnId === null
						? "Descreva a primeira tarefa…"
						: isRunning
							? "Fila: será enviado após a resposta…"
							: "Responda ou mande a próxima instrução…"
				}
			/>
		</div>
	);
}

function TopBar({
	title,
	subtitle,
	isRunning,
	combId,
	diff,
	onBack,
}: {
	title: string;
	subtitle: string | null;
	isRunning: boolean;
	combId: string | null;
	diff: WorktreeDiff | null;
	onBack: () => void;
}) {
	return (
		<header className="sticky top-0 z-10 flex items-center gap-2 border-b border-border bg-bg/90 px-3 py-2.5 backdrop-blur">
			<button
				type="button"
				onClick={onBack}
				className="-ml-1 rounded-lg p-2 text-mute hover:text-foreground"
				title="Voltar"
			>
				<ArrowLeft className="size-4" />
			</button>
			<StateDot state={isRunning ? "live" : "idle"} />
			<div className="min-w-0 flex-1">
				<p className="truncate text-[13px] font-medium leading-tight">{title}</p>
				{subtitle ? (
					<p className="flex items-center gap-1 truncate font-mono text-[10px] text-faint">
						<GitBranch className="size-2.5 shrink-0" />
						<span className="truncate">{subtitle}</span>
					</p>
				) : (
					<p className="font-mono text-[10px] uppercase tracking-wider text-faint">
						{isRunning ? "rodando" : "ocioso"}
					</p>
				)}
			</div>
			{combId && diff && !diff.clean ? (
				<Link
					to="/diff/$combId"
					params={{ combId }}
					title="Ver o que mudou"
					className="flex shrink-0 items-center gap-1.5 rounded-lg border border-border bg-panel px-2.5 py-1.5 font-mono text-[11px] tabular-nums active:bg-elevated"
				>
					<span className="text-accent">+{diff.insertions}</span>
					<span className="text-danger">−{diff.deletions}</span>
				</Link>
			) : null}
		</header>
	);
}

function workspaceLabel(meta: SessionMeta | null): string | null {
	if (!meta) return null;
	const text = [meta.workspaceName, meta.workspaceBranch].filter(Boolean).join(" · ");
	return text || null;
}

/* ── Message grouping ─────────────────────────────────────────────────────
   The raw stream interleaves conversation (user/assistant) with machinery
   (reasoning, tool calls, resolved permissions). Rendering the machinery at
   the same visual level as the conversation is what made the thread feel
   chaotic — so contiguous runs of machinery fold into one collapsible
   "trace" capsule, and the chat reads user → trace → answer again.
   Unresolved permissions stay standalone: they need the user's thumb. */

type TraceStep = Extract<
	ChatMessage,
	{ kind: "reasoning" } | { kind: "tool" } | { kind: "permission" }
>;

type RenderItem =
	| { type: "single"; key: string; message: ChatMessage }
	| { type: "trace"; key: string; steps: TraceStep[] };

function groupMessages(messages: ChatMessage[]): RenderItem[] {
	const items: RenderItem[] = [];
	let run: TraceStep[] = [];
	const flush = () => {
		if (run.length > 0) {
			const first = run[0]!;
			items.push({ type: "trace", key: `trace-${messageKey(first, 0)}`, steps: run });
			run = [];
		}
	};
	for (let i = 0; i < messages.length; i++) {
		const msg = messages[i];
		if (!msg) continue;
		const isStep =
			msg.kind === "reasoning" ||
			msg.kind === "tool" ||
			(msg.kind === "permission" && msg.resolved);
		if (isStep) {
			run.push(msg as TraceStep);
			continue;
		}
		flush();
		items.push({ type: "single", key: messageKey(msg, i), message: msg });
	}
	flush();
	return items;
}

function MessageList({
	messages,
	onRespondPermission,
}: {
	messages: ChatMessage[];
	onRespondPermission: (requestId: string, choice: string) => void;
}) {
	const items = useMemo(() => groupMessages(messages), [messages]);
	return (
		<div className="space-y-3">
			{items.map((item, i) => {
				// A user message opens a new turn — give it extra air above so
				// turns read as paragraphs, not one continuous feed.
				const opensTurn =
					item.type === "single" && item.message.kind === "user" && i > 0;
				return (
					<div key={item.key} className={cn(opensTurn && "pt-4")}>
						{item.type === "trace" ? (
							<TraceBlock steps={item.steps} />
						) : (
							<MessageView
								message={item.message}
								onRespondPermission={onRespondPermission}
							/>
						)}
					</div>
				);
			})}
		</div>
	);
}

function messageKey(msg: ChatMessage, fallbackIndex: number): string {
	switch (msg.kind) {
		case "user":
		case "assistant":
			return `${msg.kind}-${msg.turnId}`;
		case "reasoning":
			return `reasoning-${msg.id}`;
		case "tool":
			return `tool-${msg.toolCallId}`;
		case "permission":
			return `perm-${msg.requestId}`;
		case "system":
			return `sys-${msg.id}`;
		default:
			return `evt-${fallbackIndex}`;
	}
}

function MessageView({
	message,
	onRespondPermission,
}: {
	message: ChatMessage;
	onRespondPermission: (requestId: string, choice: string) => void;
}) {
	switch (message.kind) {
		case "user":
			return <UserBubble text={message.text} />;
		case "assistant":
			return (
				<AssistantBlock
					text={message.text}
					completed={message.completed}
					aborted={message.aborted}
				/>
			);
		case "permission":
			return <PermissionRow message={message} onRespond={onRespondPermission} />;
		case "system":
			return <SystemRow text={message.text} />;
		// Reasoning/tools normally render inside a TraceBlock; this is only
		// reached if one arrives outside a run (defensive fallback).
		case "reasoning":
		case "tool":
			return <TraceBlock steps={[message]} />;
	}
}

function UserBubble({ text }: { text: string }) {
	return (
		<div className="flex justify-end">
			<div className="max-w-[85%] min-w-0 overflow-hidden whitespace-pre-wrap break-words rounded-2xl rounded-br-md bg-accent px-3.5 py-2 text-[14px] leading-relaxed text-[var(--color-accent-ink)]">
				{text || <em className="opacity-60">(vazio)</em>}
			</div>
		</div>
	);
}

/** The agent's answer: full-width prose, no box. The user bubble on the
 *  right is enough to tell the two voices apart, and markdown needs the
 *  whole viewport on a phone. */
function AssistantBlock({
	text,
	completed,
	aborted,
}: {
	text: string;
	completed: boolean;
	aborted: boolean;
}) {
	return (
		<div className="min-w-0 px-0.5">
			{text ? (
				<Markdown text={text} />
			) : !completed ? (
				<Loader2 className="size-3.5 animate-spin text-mute" />
			) : null}
			{!completed && text ? (
				<span className="ml-1 inline-block size-2 animate-pulse rounded-full bg-accent align-middle" />
			) : null}
			{aborted ? (
				<p className="mt-1.5 font-mono text-[10px] uppercase tracking-[0.18em] text-danger/80">
					interrompido
				</p>
			) : null}
		</div>
	);
}

/* ── Trace capsule ──────────────────────────────────────────────────────── */

function stepIsLive(step: TraceStep): boolean {
	if (step.kind === "reasoning") return !step.completed;
	if (step.kind === "tool") return step.status === "running";
	return false;
}

function stepLabel(step: TraceStep): string {
	if (step.kind === "reasoning") return step.label;
	if (step.kind === "tool") return step.toolName;
	return firstLine(step.question) || "permissão";
}

function firstLine(text: string): string {
	return text.split("\n", 1)[0]?.trim() ?? "";
}

function TraceBlock({ steps }: { steps: TraceStep[] }) {
	const [open, setOpen] = useState(false);
	const live = steps.some(stepIsLive);
	const failedCount = steps.filter(
		(s) => s.kind === "tool" && s.status === "failed",
	).length;
	const current = live ? [...steps].reverse().find(stepIsLive) : null;
	return (
		<div className="overflow-hidden rounded-xl border border-border/60 bg-panel/50">
			<button
				type="button"
				onClick={() => setOpen((o) => !o)}
				className="flex w-full min-w-0 items-center gap-2 px-3 py-2 text-left active:bg-elevated/60"
			>
				{live ? (
					<StateDot state="live" />
				) : (
					<ChevronRight
						className={cn(
							"size-3 shrink-0 text-faint transition-transform",
							open && "rotate-90",
						)}
					/>
				)}
				<span className="shrink-0 font-mono text-[10px] font-semibold uppercase tracking-[0.18em] text-mute">
					atividade
				</span>
				<span className="grid min-w-[17px] shrink-0 place-items-center rounded-full bg-elevated px-1 font-mono text-[10px] font-bold tabular-nums text-mute">
					{steps.length}
				</span>
				{live && current ? (
					<span className="min-w-0 flex-1 truncate font-mono text-[11px] text-accent/90">
						{stepLabel(current)}
					</span>
				) : (
					<span className="flex-1" />
				)}
				{failedCount > 0 ? (
					<span className="shrink-0 font-mono text-[10px] tabular-nums text-danger">
						{failedCount} ✗
					</span>
				) : null}
				{live ? (
					<ChevronRight
						className={cn(
							"size-3 shrink-0 text-faint transition-transform",
							open && "rotate-90",
						)}
					/>
				) : null}
			</button>
			{open ? (
				<div className="space-y-0.5 border-t border-border/60 px-1.5 py-1.5">
					{steps.map((step, i) => (
						<StepRow key={messageKey(step, i)} step={step} />
					))}
				</div>
			) : null}
		</div>
	);
}

function StepRow({ step }: { step: TraceStep }) {
	const [open, setOpen] = useState(false);
	const live = stepIsLive(step);
	const failed = step.kind === "tool" && step.status === "failed";
	const Icon =
		step.kind === "reasoning" ? Brain : step.kind === "tool" ? Wrench : ShieldCheck;
	const expandable =
		step.kind === "tool" &&
		(step.input != null || step.output != null || step.error != null);
	const row = (
		<>
			<Icon className={cn("size-3 shrink-0", live ? "text-accent" : "text-faint")} />
			<span
				className={cn(
					"min-w-0 flex-1 truncate font-mono text-[11.5px]",
					failed ? "text-danger" : live ? "text-accent/90" : "text-foreground/75",
				)}
			>
				{stepLabel(step)}
			</span>
			{live ? (
				<Loader2 className="size-3 shrink-0 animate-spin text-accent" />
			) : failed ? (
				<X className="size-3 shrink-0 text-danger" />
			) : (
				<Check className="size-3 shrink-0 text-faint" />
			)}
		</>
	);
	if (!expandable) {
		return <div className="flex items-center gap-2 rounded-lg px-1.5 py-1.5">{row}</div>;
	}
	return (
		<div>
			<button
				type="button"
				onClick={() => setOpen((o) => !o)}
				className="flex w-full min-w-0 items-center gap-2 rounded-lg px-1.5 py-1.5 text-left active:bg-elevated"
			>
				{row}
			</button>
			{open && step.kind === "tool" ? (
				<div className="mb-1 ml-5 space-y-1.5 pr-1.5">
					{step.input !== undefined && step.input !== null ? (
						<Snippet label="input" value={step.input} />
					) : null}
					{step.output ? <Snippet label="output" value={step.output} /> : null}
					{step.error ? (
						<p className="font-mono text-[11px] text-danger">{step.error}</p>
					) : null}
				</div>
			) : null}
		</div>
	);
}

function Snippet({ label, value }: { label: string; value: unknown }) {
	const text =
		typeof value === "string" ? value : JSON.stringify(value, null, 2);
	return (
		<div>
			<p className="mb-1 font-mono text-[9px] uppercase tracking-wider text-faint">
				{label}
			</p>
			<pre className="max-h-48 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-bg px-2 py-1.5 font-mono text-[11px] text-foreground/90">
				{text}
			</pre>
		</div>
	);
}

function PermissionRow({
	message,
	onRespond,
}: {
	message: Extract<ChatMessage, { kind: "permission" }>;
	onRespond: (requestId: string, choice: string) => void;
}) {
	const disabled = message.resolved;
	return (
		<div className="rounded-2xl border border-wait/40 bg-wait/[0.06] p-3 text-[13px]">
			<div className="flex items-start gap-2">
				<AlertTriangle className="mt-0.5 size-4 shrink-0 text-wait" />
				<div className="min-w-0 flex-1">
					<p className="font-mono text-[10px] font-semibold uppercase tracking-[0.18em] text-wait">
						Precisa de você
					</p>
					<p className="mt-1.5 whitespace-pre-wrap text-[13px] leading-relaxed text-foreground/90">
						{message.question}
					</p>
				</div>
			</div>
			<div className="mt-3 flex flex-wrap gap-2">
				{message.choices.map((choice) => (
					<button
						key={choice.id}
						type="button"
						disabled={disabled}
						onClick={() => onRespond(message.requestId, choice.id)}
						className={cn(
							"flex-1 rounded-lg px-3 py-2 text-[13px] font-semibold",
							choice.id === "deny"
								? "border border-border bg-bg text-foreground active:bg-elevated"
								: "bg-wait text-[var(--color-wait-ink)] active:opacity-80",
							disabled && "opacity-50",
						)}
					>
						{choice.label}
					</button>
				))}
			</div>
			{disabled ? (
				<p className="mt-2 font-mono text-[10px] uppercase tracking-wider text-faint">
					Resolvido
				</p>
			) : null}
		</div>
	);
}

function SystemRow({ text }: { text: string }) {
	return (
		<div className="flex items-center gap-3 px-4 py-1">
			<span className="h-px flex-1 bg-border/60" />
			<span className="font-mono text-[10px] uppercase tracking-[0.18em] text-faint">
				{text}
			</span>
			<span className="h-px flex-1 bg-border/60" />
		</div>
	);
}

function Composer({
	value,
	onChange,
	onSubmit,
	sending,
	running,
	aborting,
	onAbort,
	placeholder,
}: {
	value: string;
	onChange: (v: string) => void;
	onSubmit: () => void;
	sending: boolean;
	running: boolean;
	aborting: boolean;
	onAbort: () => void;
	placeholder: string;
}) {
	const ref = useRef<HTMLTextAreaElement | null>(null);
	const grow = (el: HTMLTextAreaElement) => {
		el.style.height = "auto";
		el.style.height = `${Math.min(el.scrollHeight, 140)}px`;
	};
	return (
		<footer className="sticky bottom-0 border-t border-border bg-bg/95 px-3 pb-3 pt-2 backdrop-blur">
			<div className="mx-auto max-w-md">
				{running ? (
					<div className="mb-2 flex items-center gap-2 px-1.5">
						<StateDot state="live" />
						<span className="flex-1 font-mono text-[11px] text-mute">
							Agente trabalhando…
						</span>
						<button
							type="button"
							onClick={onAbort}
							disabled={aborting}
							className="inline-flex items-center gap-1.5 rounded-lg border border-danger/30 bg-danger/10 px-2.5 py-1 text-[11px] font-medium text-danger active:bg-danger/15 disabled:opacity-50"
						>
							{aborting ? (
								<Loader2 className="size-3 animate-spin" />
							) : (
								<StopCircle className="size-3" />
							)}
							Parar
						</button>
					</div>
				) : null}
				<div className="flex items-end gap-1.5 rounded-2xl border border-border bg-panel p-1.5 transition-colors focus-within:border-accent/50">
					<textarea
						ref={ref}
						value={value}
						onChange={(e) => {
							onChange(e.target.value);
							grow(e.currentTarget);
						}}
						onKeyDown={(e) => {
							if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
								e.preventDefault();
								onSubmit();
							}
						}}
						placeholder={placeholder}
						rows={1}
						className="min-w-0 flex-1 resize-none bg-transparent px-2.5 py-2 text-[14px] leading-snug text-foreground outline-none placeholder:text-faint focus:outline-none focus-visible:outline-none"
						style={{ minHeight: 38, maxHeight: 140 }}
					/>
					<button
						type="button"
						onClick={onSubmit}
						disabled={sending || value.trim().length === 0}
						className="grid size-[38px] shrink-0 place-items-center rounded-xl bg-accent text-[var(--color-accent-ink)] transition-opacity disabled:opacity-40"
						title="Enviar"
					>
						{sending ? (
							<Loader2 className="size-4 animate-spin" />
						) : (
							<Send className="size-4" />
						)}
					</button>
				</div>
			</div>
		</footer>
	);
}

function ErrorBanner({
	message,
	onDismiss,
}: {
	message: string;
	onDismiss: () => void;
}) {
	return (
		<div className="rounded-2xl border border-danger/30 bg-danger/5 p-3 text-[13px] text-danger">
			<div className="flex items-start gap-2">
				<AlertTriangle className="mt-0.5 size-4 shrink-0" />
				<p className="flex-1">{message}</p>
				<button
					type="button"
					onClick={onDismiss}
					className="text-mute hover:text-foreground"
					aria-label="dismiss"
				>
					×
				</button>
			</div>
		</div>
	);
}
