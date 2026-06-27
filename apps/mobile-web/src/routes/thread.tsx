import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import {
	AlertTriangle,
	ArrowLeft,
	Brain,
	ChevronDown,
	ChevronUp,
	GitBranch,
	Loader2,
	Send,
	StopCircle,
	Wrench,
} from "lucide-react";
import { Markdown } from "@/components/Markdown";
import { Shell, StateDot } from "@/components/ui";
import { apiFetch } from "@/lib/api";
import { foldEntry, type BundleEntry, type WorktreeDiff } from "@/lib/diff";
import { cn } from "@/lib/cn";
import { loadSession, type PairingSession } from "@/lib/session";
import { openEventStream } from "@/lib/sseClient";
import {
	applyEvents,
	createThreadState,
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
				if (!payload || typeof payload !== "object") return;
				const event = payload as RawSessionEvent;
				if (event.sessionId !== threadId) return;
				setState((prev) => applyEvents(prev, [event]));
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
	}, [state.messages.length]);

	const lastTurnId = useMemo(() => {
		for (let i = state.messages.length - 1; i >= 0; i--) {
			const m = state.messages[i];
			if (m && "turnId" in m && m.turnId) return m.turnId;
		}
		return null;
	}, [state.messages]);

	const lastAssistant = useMemo(() => {
		for (let i = state.messages.length - 1; i >= 0; i--) {
			const m = state.messages[i];
			if (m?.kind === "assistant") return m;
		}
		return null;
	}, [state.messages]);

	const isRunning =
		lastAssistant !== null && !lastAssistant.completed && !lastAssistant.aborted;

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
			await apiFetch(session, `/api/v1/sessions/${encodeURIComponent(threadId)}/turns`, {
				method: "POST",
				body: JSON.stringify({ sessionId: threadId, prompt: text }),
			});
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
						choice,
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
				onAbort={isRunning ? () => void abort() : null}
				aborting={aborting}
			/>

			<div ref={scrollerRef} className="flex-1 overflow-y-auto">
				<div className="mx-auto max-w-md px-4 py-4">
					{loading ? (
						<div className="flex h-[40vh] items-center justify-center text-mute">
							<Loader2 className="size-5 animate-spin" />
						</div>
					) : loadError ? (
						<ErrorBanner message={loadError} onDismiss={() => setLoadError(null)} />
					) : state.messages.length === 0 ? (
						<p className="rounded-2xl border border-dashed border-border/70 p-6 text-center text-[12px] text-mute">
							Sem mensagens ainda. Mande um prompt abaixo.
						</p>
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
				disabled={sending || !!loadError}
				placeholder={
					lastTurnId === null
						? "Envie a primeira mensagem…"
						: isRunning
							? "Aguardando resposta…"
							: "Digite uma instrução…"
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
	onAbort,
	aborting,
}: {
	title: string;
	subtitle: string | null;
	isRunning: boolean;
	combId: string | null;
	diff: WorktreeDiff | null;
	onBack: () => void;
	onAbort: (() => void) | null;
	aborting: boolean;
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
			{onAbort ? (
				<button
					type="button"
					onClick={onAbort}
					disabled={aborting}
					className="inline-flex items-center gap-1.5 rounded-lg border border-danger/30 bg-danger/10 px-2.5 py-1.5 text-[11px] font-medium text-danger active:bg-danger/15 disabled:opacity-50"
				>
					{aborting ? (
						<Loader2 className="size-3 animate-spin" />
					) : (
						<StopCircle className="size-3" />
					)}
					Parar
				</button>
			) : null}
		</header>
	);
}

function workspaceLabel(meta: SessionMeta | null): string | null {
	if (!meta) return null;
	const text = [meta.workspaceName, meta.workspaceBranch].filter(Boolean).join(" · ");
	return text || null;
}

function MessageList({
	messages,
	onRespondPermission,
}: {
	messages: ChatMessage[];
	onRespondPermission: (requestId: string, choice: string) => void;
}) {
	return (
		<div className="space-y-3">
			{messages.map((msg, i) => (
				<MessageView
					key={messageKey(msg, i)}
					message={msg}
					onRespondPermission={onRespondPermission}
				/>
			))}
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
			return <AssistantBubble text={message.text} completed={message.completed} aborted={message.aborted} />;
		case "reasoning":
			return <ReasoningRow label={message.label} completed={message.completed} />;
		case "tool":
			return <ToolRow message={message} />;
		case "permission":
			return <PermissionRow message={message} onRespond={onRespondPermission} />;
		case "system":
			return <SystemRow text={message.text} />;
	}
}

function UserBubble({ text }: { text: string }) {
	return (
		<div className="flex justify-end">
			<div className="max-w-[85%] rounded-2xl rounded-br-md bg-accent px-3.5 py-2 text-[14px] leading-relaxed text-[var(--color-accent-ink)]">
				{text || <em className="opacity-60">(vazio)</em>}
			</div>
		</div>
	);
}

function AssistantBubble({
	text,
	completed,
	aborted,
}: {
	text: string;
	completed: boolean;
	aborted: boolean;
}) {
	return (
		<div className="flex justify-start">
			<div className="max-w-[90%] rounded-2xl rounded-bl-md border border-border bg-panel px-3.5 py-2 text-foreground">
				{text ? (
					<Markdown text={text} />
				) : (
					<Loader2 className="size-3.5 animate-spin text-mute" />
				)}
				{!completed && text ? (
					<span className="ml-1 inline-block size-2 animate-pulse rounded-full bg-accent align-middle" />
				) : null}
				{aborted ? (
					<p className="mt-2 text-[11px] uppercase tracking-wider text-mute">abortado</p>
				) : null}
			</div>
		</div>
	);
}

function ReasoningRow({ label, completed }: { label: string; completed: boolean }) {
	return (
		<div className="flex items-center gap-2 px-1 font-mono text-[11px] text-faint">
			<Brain className="size-3" />
			<span className="italic">{label}</span>
			{!completed ? <Loader2 className="size-3 animate-spin" /> : null}
		</div>
	);
}

function ToolRow({
	message,
}: {
	message: Extract<ChatMessage, { kind: "tool" }>;
}) {
	const [open, setOpen] = useState(false);
	const palette =
		message.status === "running"
			? "border-accent/30 bg-accent/5 text-accent"
			: message.status === "failed"
				? "border-danger/30 bg-danger/5 text-danger"
				: "border-border bg-elevated text-mute";
	return (
		<div className={cn("rounded-xl border px-3 py-2 text-[12px]", palette)}>
			<button
				type="button"
				onClick={() => setOpen((o) => !o)}
				className="flex w-full items-center gap-2 text-left"
			>
				<Wrench className="size-3.5" />
				<span className="flex-1 font-mono text-[12px]">{message.toolName}</span>
				<span className="text-[10px] uppercase tracking-wider">
					{message.status}
				</span>
				{open ? <ChevronUp className="size-3.5" /> : <ChevronDown className="size-3.5" />}
			</button>
			{open ? (
				<div className="mt-2 space-y-2 border-t border-current/10 pt-2">
					{message.input !== undefined && message.input !== null ? (
						<Snippet label="input" value={message.input} />
					) : null}
					{message.output ? <Snippet label="output" value={message.output} /> : null}
					{message.error ? (
						<p className="font-mono text-[11px] text-danger">{message.error}</p>
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
			<p className="mb-1 text-[9px] uppercase tracking-wider opacity-70">{label}</p>
			<pre className="max-h-48 overflow-auto whitespace-pre-wrap break-words rounded bg-bg px-2 py-1.5 font-mono text-[11px] text-foreground/90">
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
					<p className="mt-1.5 text-[13px] leading-relaxed text-foreground/90">
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
		<p className="text-center text-[10px] uppercase tracking-wider text-mute/60">
			— {text} —
		</p>
	);
}

function Composer({
	value,
	onChange,
	onSubmit,
	disabled,
	placeholder,
}: {
	value: string;
	onChange: (v: string) => void;
	onSubmit: () => void;
	disabled: boolean;
	placeholder: string;
}) {
	const ref = useRef<HTMLTextAreaElement | null>(null);
	const grow = (el: HTMLTextAreaElement) => {
		el.style.height = "auto";
		el.style.height = `${Math.min(el.scrollHeight, 140)}px`;
	};
	return (
		<footer className="sticky bottom-0 border-t border-border bg-bg/90 px-3 py-3 backdrop-blur">
			<div className="mx-auto flex max-w-md items-end gap-2">
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
					className="flex-1 resize-none rounded-2xl border border-border bg-panel px-3.5 py-2.5 text-[14px] leading-snug text-foreground outline-none placeholder:text-faint focus:border-accent/50"
					style={{ minHeight: 42, maxHeight: 140 }}
				/>
				<button
					type="button"
					onClick={onSubmit}
					disabled={disabled || value.trim().length === 0}
					className="grid size-[42px] shrink-0 place-items-center rounded-2xl bg-accent text-[var(--color-accent-ink)] transition-opacity disabled:opacity-40"
					title="Enviar"
				>
					{disabled ? (
						<Loader2 className="size-4 animate-spin" />
					) : (
						<Send className="size-4" />
					)}
				</button>
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

