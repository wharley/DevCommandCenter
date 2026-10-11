/**
 * Offline demo for people without a paired desktop (and for App Review, which
 * has no Mac to pair with). It answers the same API paths the real desktop
 * does, with fictional projects, and plays an agent that streams, asks for a
 * permission and finishes — so every screen works exactly as when paired.
 */
import {
	base64Size,
	filterTrackedFiles,
	MAX_ATTACHMENT_BYTES,
	taskTitle,
	type CatalogRepository,
	type Comb,
	type MobileCatalog,
	type RawSessionEvent,
	type SessionSearchResult,
	type TaskProgress,
} from "@dcc/mobile-core";
import { ApiError, type Backend, type ConnectionState, type StreamHandlers } from "./types";

type DemoSession = SessionSearchResult & { events: RawSessionEvent[] };

/** The SSE envelope name of each event kind, as the desktop emits it. */
const CORE_NAMES: Record<string, string> = {
	turn_started: "sessionTurnStarted",
	turn_delta: "sessionTurnDelta",
	turn_completed: "sessionTurnCompleted",
	turn_aborted: "sessionTurnAborted",
	turn_reasoning_started: "sessionTurnReasoningStarted",
	turn_reasoning_completed: "sessionTurnReasoningCompleted",
	turn_tool_call_started: "sessionTurnToolCallStarted",
	turn_tool_call_completed: "sessionTurnToolCallCompleted",
	turn_permission_requested: "sessionTurnPermissionRequested",
	turn_permission_resolved: "sessionTurnPermissionResolved",
	turn_user_input_requested: "sessionTurnUserInputRequested",
	turn_user_input_resolved: "sessionTurnUserInputResolved",
};

type Step = { delayMs: number; kind: RawSessionEvent["kind"] };

export class DemoBackend implements Backend {
	readonly mode = "demo" as const;
	private readonly sessions: DemoSession[];
	private readonly combs: Comb[];
	private readonly repositories: CatalogRepository[];
	/** Task creations by request id, as the desktop journals them. */
	private readonly tasks = new Map<string, { input: string; result: TaskProgress }>();
	/** Uploaded images by path (the desktop keeps them in its temp dir). */
	private readonly attachments = new Map<string, string>();
	private attachmentCount = 0;
	private readonly subs = new Set<{ path: () => string; handlers: StreamHandlers }>();
	private readonly timers = new Set<ReturnType<typeof setTimeout>>();
	/** Steps that resume once a permission is answered, by request id. */
	private readonly parked = new Map<string, { sessionId: string; turnId: string; steps: Step[] }>();
	private eventCounter = 0;

	constructor(now = Date.now()) {
		const fixture = demoFixture(now);
		this.sessions = fixture.sessions;
		this.combs = fixture.combs;
		this.repositories = fixture.repositories;
		this.eventCounter = 1000;
		// The checkout session starts blocked on a permission; allowing it
		// finishes the turn like a live agent would.
		this.parked.set("demo-p1", {
			sessionId: "demo-s1",
			turnId: "b1",
			steps: [
				{ delayMs: 200, kind: { type: "turn_tool_call_started", turnId: "b1", toolCallId: "b1t3", toolName: "Bash", toolInput: { command: "yarn test checkout" } } },
				{ delayMs: 1500, kind: { type: "turn_tool_call_completed", turnId: "b1", toolCallId: "b1t3" } },
				...streamText("b1", "Migração feita: o checkout usa o novo gateway e os 18 testes do módulo passaram. Mantive o gateway antigo atrás de uma flag para rollback."),
				{ delayMs: 300, kind: { type: "turn_completed", turnId: "b1" } },
			],
		});
	}

	state(): ConnectionState {
		return { kind: "online" };
	}

	onState(): () => void {
		return () => {};
	}

	reconnectNow(): void {}

	dispose(): void {
		for (const timer of this.timers) clearTimeout(timer);
		this.timers.clear();
		this.subs.clear();
	}

	subscribe(path: () => string, handlers: StreamHandlers): () => void {
		const sub = { path, handlers };
		this.subs.add(sub);
		const url = new URL(path(), "http://demo");
		const sessionId = url.searchParams.get("sessionId");
		const after = Number(url.searchParams.get("after") ?? "NaN");
		this.later(0, () => {
			if (!this.subs.has(sub)) return;
			handlers.onOpen?.(false);
			const session = sessionId ? this.find(sessionId) : null;
			if (session && Number.isFinite(after)) {
				for (const event of session.events.filter((e) => e.sequence > after)) {
					handlers.onEvent({ event: "replay", data: event, seq: event.sequence });
				}
			}
		});
		return () => this.subs.delete(sub);
	}

	async request<T>(method: string, path: string, body?: unknown): Promise<T> {
		await new Promise((resolve) => this.later(120, () => resolve(null)));
		return this.route(method.toUpperCase(), new URL(path, "http://demo"), body) as T;
	}

	/** Like the desktop's route: images by content, a size limit, a path back. */
	private saveAttachment(body: unknown) {
		const input = (body ?? {}) as { name?: unknown; data?: unknown };
		const data = typeof input.data === "string" ? input.data : "";
		const size = base64Size(data);
		if (size > MAX_ATTACHMENT_BYTES) throw new ApiError(400, "O anexo passa de 10 MB.");
		if (size === 0) throw new ApiError(400, "O anexo está vazio.");
		const id = ++this.attachmentCount;
		const image = data.startsWith("/9j/") ? "jpg" : data.startsWith("iVBOR") ? "png" : null;
		const name = String(input.name ?? "arquivo").replace(/[\s/\\<>]+/g, "-") || "arquivo.txt";
		const path = image ? `/demo/tmp/dcc_img_${id}.${image}` : `/demo/tmp/dcc_file_${id}/${name}`;
		if (image) this.attachments.set(path, `data:image/${image === "jpg" ? "jpeg" : "png"};base64,${data}`);
		return { kind: image ? "image" : "file", path, name: path.split("/").pop(), size };
	}

	private attachmentPreview(sessionId: string, path: string) {
		const session = this.find(sessionId);
		const referenced = session?.events.some(
			(e) => e.kind.type === "turn_started" && typeof e.kind.prompt === "string" && e.kind.prompt.includes(`@${path}`),
		);
		const dataUrl = referenced ? this.attachments.get(path) : undefined;
		if (!dataUrl) throw new ApiError(404, "Prévia indisponível.");
		return { dataUrl };
	}

	private route(method: string, url: URL, body: unknown): unknown {
		const path = url.pathname;
		if (method === "GET" && path === "/api/v1/status") {
			return { running: true, cpuPercent: 7, memoryMb: 412 };
		}
		if (method === "GET" && path === "/api/v1/combs") return this.combs;
		if (method === "GET" && path === "/api/v1/mobile/catalog") return this.catalog();
		if (method === "POST" && path === "/api/v1/mobile/tasks") return this.createTask(body);
		if (method === "POST" && path === "/api/v1/mobile/attachments") return this.saveAttachment(body);
		const preview = /^\/api\/v1\/mobile\/sessions\/([^/]+)\/attachment$/.exec(path);
		if (method === "GET" && preview) {
			return this.attachmentPreview(decodeURIComponent(preview[1]!), url.searchParams.get("path") ?? "");
		}
		const files = /^\/api\/v1\/mobile\/(workspaces|repositories)\/([^/]+)\/files$/.exec(path);
		if (method === "GET" && files) return this.files(files[1]!, decodeURIComponent(files[2]!), url.searchParams.get("q") ?? "");
		const task = /^\/api\/v1\/mobile\/tasks\/([^/]+)$/.exec(path);
		if (method === "GET" && task) {
			const entry = this.tasks.get(decodeURIComponent(task[1]!));
			if (!entry) throw new ApiError(404, "Criação não encontrada.");
			return entry.result;
		}
		if (method === "GET" && path === "/api/v1/sessions/search") {
			return [...this.sessions]
				.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
				.map(({ events: _events, ...row }) => row);
		}
		const match = /^\/api\/v1\/sessions\/([^/]+)\/([a-z-]+)$/.exec(path);
		if (!match) throw new ApiError(404, "Não existe no modo demonstração.");
		const session = this.find(decodeURIComponent(match[1]!));
		if (!session) throw new ApiError(404, "Sessão não encontrada.");
		const action = match[2];
		const input = (body ?? {}) as Record<string, unknown>;

		if (method === "GET" && action === "events") {
			const after = url.searchParams.get("after");
			if (after === null) return session.events;
			const cursor = Number(after);
			const limit = Number(url.searchParams.get("limit") ?? "500");
			const newer = session.events.filter((e) => e.sequence > cursor);
			return {
				events: newer.slice(0, limit),
				lastSeq: session.events.at(-1)?.sequence ?? 0,
				hasMore: newer.length > limit,
			};
		}
		if (method === "POST" && action === "turns") {
			const prompt = typeof input.prompt === "string" ? input.prompt : "";
			// Like the desktop: without a pick, the session keeps its latest level.
			this.play(session, prompt, policyOf(input.approvalPolicy) ?? latestPolicy(session));
			return { turn: { content: prompt } };
		}
		if (method === "POST" && action === "abort") {
			const turnId = this.openTurn(session);
			if (turnId) {
				for (const [id, parked] of this.parked) if (parked.sessionId === session.sessionId) this.parked.delete(id);
				this.append(session, { type: "turn_aborted", turnId, reason: "user" });
			}
			return { ok: true };
		}
		if (method === "POST" && action === "respond-permission") {
			const requestId = String(input.requestId ?? "");
			const behavior = String(input.behavior ?? "deny");
			const resolved = session.events.find(
				(e) => e.kind.type === "turn_permission_resolved" && e.kind.requestId === requestId,
			);
			if (resolved) {
				throw new ApiError(409, "Já foi respondido em outro lugar.", {
					ok: false,
					error: { code: "already_resolved", outcome: { kind: "permission", behavior: resolved.kind.behavior } },
				});
			}
			const requested = session.events.find(
				(e) => e.kind.type === "turn_permission_requested" && e.kind.requestId === requestId,
			);
			if (!requested) throw new ApiError(404, "Pedido não encontrado.");
			const turnId = requested.kind.turnId ?? "";
			this.append(session, { type: "turn_permission_resolved", turnId, requestId, behavior });
			const parked = this.parked.get(requestId);
			this.parked.delete(requestId);
			if (behavior === "deny") {
				this.schedule(session, [
					{ delayMs: 400, kind: { type: "turn_delta", turnId, content: "Ok, não vou rodar isso. Paro por aqui." } },
					{ delayMs: 300, kind: { type: "turn_completed", turnId } },
				]);
			} else if (parked) {
				this.schedule(session, parked.steps);
			}
			return { ok: true };
		}
		if (method === "POST" && action === "respond-user-input") {
			const requestId = String(input.requestId ?? "");
			const already = session.events.some(
				(e) => e.kind.type === "turn_user_input_resolved" && e.kind.requestId === requestId,
			);
			if (already) throw new ApiError(409, "Já foi respondido em outro lugar.");
			const requested = session.events.find(
				(e) => e.kind.type === "turn_user_input_requested" && e.kind.requestId === requestId,
			);
			const turnId = requested?.kind.turnId ?? "";
			const answers = Array.isArray(input.answers) ? (input.answers as Array<{ answer?: string }>) : [];
			this.append(session, { type: "turn_user_input_resolved", turnId, requestId });
			this.schedule(session, [
				...streamText(turnId, `\n\nCombinado: ${answers.map((a) => a.answer).filter(Boolean).join(", ") || "sigo com o padrão"}. Atualizei o plano e sigo daqui.`),
				{ delayMs: 300, kind: { type: "turn_completed", turnId } },
			]);
			return { ok: true };
		}
		throw new ApiError(404, "Não existe no modo demonstração.");
	}

	private catalog(): MobileCatalog {
		return {
			providers: DEMO_PROVIDERS,
			repositories: this.repositories,
			workspaces: this.combs.map((c) => ({
				id: c.id,
				projectId: c.projectId ?? "",
				name: c.name,
				rootPath: c.projectPath ?? "",
				baseBranch: c.branch ?? "main",
				worktreePath: c.worktreePath,
				state: c.status ?? "ready",
				updatedAt: c.lastOpenedAt ?? undefined,
			})),
		};
	}

	/** Fictional tracked files of a project, filtered like the desktop does. */
	private files(scope: string, id: string, query: string) {
		const rootPath =
			scope === "workspaces"
				? this.combs.find((c) => c.id === id)?.projectPath
				: this.repositories.find((r) => r.id === id)?.rootPath;
		if (!rootPath) throw new ApiError(404, scope === "workspaces" ? "Workspace não encontrado." : "Repositório não encontrado no DCC.");
		const paths = demoFiles(rootPath);
		return { files: filterTrackedFiles(paths, query).map((path) => ({ path })), total: paths.length };
	}

	/** Same contract as the desktop: idempotent by `requestId`, a new workspace per task. */
	private createTask(body: unknown): TaskProgress {
		const input = (body ?? {}) as Record<string, unknown>;
		const requestId = String(input.requestId ?? "");
		const prompt = typeof input.prompt === "string" ? input.prompt.trim() : "";
		const encoded = JSON.stringify(input);
		const known = this.tasks.get(requestId);
		if (known) {
			if (known.input !== encoded) throw new ApiError(400, "Esta criação já foi usada com outra descrição.");
			return known.result;
		}
		if (!requestId || !prompt) throw new ApiError(400, "Descreva a tarefa.");
		const repo = this.repositories.find((r) => r.id === input.repositoryId);
		if (!repo) throw new ApiError(404, "Repositório não encontrado no DCC.");
		const provider = DEMO_PROVIDERS.find((p) => p.id === input.providerId);
		if (!provider) throw new ApiError(400, "Agente desconhecido.");
		const title = typeof input.title === "string" && input.title.trim() ? input.title.trim() : taskTitle(prompt);
		const worktree = input.isolationMode !== "localDirect";
		const slug = title.toLowerCase().normalize("NFD").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 32) || "tarefa";
		const now = new Date().toISOString();
		const comb: Comb = {
			id: `demo-w${++this.eventCounter}`,
			name: title,
			branch: worktree ? `dcc/${slug}` : repo.baseBranch,
			projectId: repo.projectId,
			projectName: repo.name,
			projectPath: repo.rootPath,
			projectColor: repo.color ?? null,
			worktreePath: worktree ? `${repo.rootPath}/.worktrees/${slug}` : repo.rootPath,
			status: "ready",
			lastOpenedAt: now,
		};
		this.combs.unshift(comb);
		const session: DemoSession = {
			sessionId: `demo-s${++this.eventCounter}`,
			threadTitle: title,
			snippet: null,
			providerId: provider.id,
			model: typeof input.model === "string" ? input.model : null,
			workspaceName: comb.name,
			workspaceBranch: comb.branch,
			workspaceId: comb.id,
			projectId: repo.name,
			updatedAt: now,
			archivedAt: null,
			events: [],
		};
		this.sessions.push(session);
		this.append(session, { type: "session_started" });
		this.play(session, prompt, policyOf(input.approvalPolicy) ?? "ask");
		this.tasks.set(requestId, {
			input: encoded,
			result: { state: "started", workspaceId: comb.id, sessionId: session.sessionId },
		});
		// Like the desktop, the first answer only acknowledges; the poll brings the outcome.
		return { state: "creating" };
	}

	/**
	 * A short scripted turn: think, read, run the tests, answer. Asking for
	 * approval ("Solicitar aprovação", or no level) parks the run on a
	 * permission; the other levels run the tests straight away.
	 */
	private play(session: DemoSession, prompt: string, policy: string | null) {
		const turnId = `demo-turn-${++this.eventCounter}`;
		const requestId = `demo-perm-${this.eventCounter}`;
		this.append(session, { type: "turn_started", turnId, prompt, ...(policy ? { approvalPolicy: policy } : {}) });
		const finish: Step[] = [
			{ delayMs: 200, kind: { type: "turn_tool_call_started", turnId, toolCallId: `${turnId}-t2`, toolName: "Bash", toolInput: { command: "yarn test" } } },
			{ delayMs: 1400, kind: { type: "turn_tool_call_completed", turnId, toolCallId: `${turnId}-t2` } },
			...streamText(turnId, "Pronto: os 42 testes passaram. Fiz a mudança em `src/app.ts` e deixei o diff no workspace para você revisar."),
			{ delayMs: 300, kind: { type: "turn_completed", turnId } },
		];
		const start: Step[] = [
			{ delayMs: 500, kind: { type: "turn_reasoning_started", turnId, reasoningId: `${turnId}-r`, label: "Pensando" } },
			{ delayMs: 900, kind: { type: "turn_reasoning_completed", turnId, reasoningId: `${turnId}-r` } },
			{ delayMs: 200, kind: { type: "turn_tool_call_started", turnId, toolCallId: `${turnId}-t1`, toolName: "Read", toolInput: { file: "src/app.ts" } } },
			{ delayMs: 700, kind: { type: "turn_tool_call_completed", turnId, toolCallId: `${turnId}-t1` } },
		];
		if (policy && policy !== "ask") {
			this.schedule(session, [...start, ...finish]);
			return;
		}
		this.schedule(session, [
			...start,
			{
				delayMs: 400,
				kind: {
					type: "turn_permission_requested",
					turnId,
					requestId,
					question: "Rodar comando\nyarn test",
					choices: [
						{ id: "allow", label: "Permitir" },
						{ id: "deny", label: "Negar" },
					],
				},
			},
		]);
		this.parked.set(requestId, { sessionId: session.sessionId, turnId, steps: finish });
	}

	private schedule(session: DemoSession, steps: Step[]) {
		let at = 0;
		for (const step of steps) {
			at += step.delayMs;
			this.later(at, () => {
				// An abort drops whatever the turn still had queued.
				const turnId = step.kind.turnId;
				if (turnId && !this.turnOpen(session, turnId)) return;
				this.append(session, step.kind);
			});
		}
	}

	private turnOpen(session: DemoSession, turnId: string): boolean {
		return !session.events.some(
			(e) => e.kind.turnId === turnId && (e.kind.type === "turn_completed" || e.kind.type === "turn_aborted"),
		);
	}

	private openTurn(session: DemoSession): string | null {
		for (let i = session.events.length - 1; i >= 0; i--) {
			const kind = session.events[i]!.kind;
			if (kind.type === "turn_completed" || kind.type === "turn_aborted") return null;
			if (kind.type === "turn_started") return kind.turnId ?? null;
		}
		return null;
	}

	private append(session: DemoSession, kind: RawSessionEvent["kind"]) {
		const sequence = (session.events.at(-1)?.sequence ?? 0) + 1;
		const event: RawSessionEvent = {
			eventId: `demo-${++this.eventCounter}`,
			sessionId: session.sessionId,
			sequence,
			occurredAt: new Date().toISOString(),
			kind,
		};
		session.events.push(event);
		session.updatedAt = event.occurredAt;
		const name = CORE_NAMES[kind.type];
		if (!name) return;
		const data = {
			[name]: {
				session_id: session.sessionId,
				turn_id: kind.turnId,
				request_id: kind.requestId,
				content: kind.content,
			},
		};
		for (const sub of this.subs) {
			const filter = new URL(sub.path(), "http://demo").searchParams.get("sessionId");
			if (filter && filter !== session.sessionId) continue;
			sub.handlers.onEvent({ event: "core-event", data, seq: sequence });
		}
	}

	private find(sessionId: string): DemoSession | null {
		return this.sessions.find((s) => s.sessionId === sessionId) ?? null;
	}

	private later(ms: number, run: () => void) {
		const timer = setTimeout(() => {
			this.timers.delete(timer);
			run();
		}, ms);
		this.timers.add(timer);
	}
}

function policyOf(value: unknown): string | null {
	return typeof value === "string" && value ? value : null;
}

function latestPolicy(session: DemoSession): string | null {
	for (let i = session.events.length - 1; i >= 0; i--) {
		const kind = session.events[i]!.kind;
		if (kind.type === "turn_started" && typeof kind.approvalPolicy === "string") return kind.approvalPolicy;
	}
	return null;
}

/** A plausible tree per demo project, for `@`. */
function demoFiles(rootPath: string): string[] {
	const common = ["README.md", "package.json", "tsconfig.json", ".github/workflows/ci.yml", "src/app.ts", "src/config.ts"];
	const byProject: Record<string, string[]> = {
		"/demo/loja-online": ["src/checkout/gateway.ts", "src/checkout/gateway.test.ts", "src/checkout/cart.tsx", "src/vitrine/cache.ts", "src/vitrine/VitrinePage.tsx"],
		"/demo/api-pagamentos": ["src/auth/login.ts", "src/auth/login.test.ts", "src/http/client.ts", "src/pix/qrcode.ts"],
		"/demo/site": ["docs/onboarding.md", "docs/instalacao.md", "src/pages/index.tsx", "src/pages/contato.tsx"],
		"/demo/app-mobile": ["app/index.tsx", "app/thread/[id].tsx", "src/ui/Button.tsx", "src/theme.ts"],
	};
	return [...common, ...(byProject[rootPath] ?? [])].sort();
}

function streamText(turnId: string, text: string): Step[] {
	const chunks = text.match(/\s*\S+/g) ?? [text];
	const steps: Step[] = [];
	for (let i = 0; i < chunks.length; i += 3) {
		steps.push({ delayMs: 90, kind: { type: "turn_delta", turnId, content: chunks.slice(i, i + 3).join("") } });
	}
	return steps;
}

const DEMO_PROVIDERS: MobileCatalog["providers"] = [
	{
		id: "claude",
		label: "Claude",
		health: "Healthy",
		stable: true,
		capabilities: { approvalPolicies: ["read_only", "ask", "auto", "full_access"] },
		models: [
			{ id: "claude-opus-5-5", label: "Opus 5.5", recommended: true },
			{ id: "claude-sonnet-5-5", label: "Sonnet 5.5" },
		],
	},
	{
		id: "codex",
		label: "Codex",
		health: "Healthy",
		stable: true,
		capabilities: { approvalPolicies: ["read_only", "ask", "auto", "full_access"] },
		models: [{ id: "gpt-5.5-codex", label: "GPT-5.5 Codex", recommended: true }],
	},
	{
		id: "gemini",
		label: "Gemini",
		health: "Healthy",
		stable: false,
		models: [{ id: "gemini-3-pro", label: "Gemini 3 Pro" }],
		capabilities: { approvalPolicies: ["ask", "full_access"] },
	},
	{ id: "cursor", label: "Cursor", health: { Unhealthy: { reason: "Faça login no Cursor" } }, stable: false, models: [] },
];

function demoFixture(now: number): { sessions: DemoSession[]; combs: Comb[]; repositories: CatalogRepository[] } {
	const ago = (minutes: number) => new Date(now - minutes * 60_000).toISOString();
	const combs: Comb[] = [
		{ id: "demo-w1", name: "checkout-v2", branch: "feat/checkout-v2", projectId: "loja", projectName: "loja-online", projectPath: "/demo/loja-online", projectColor: "emerald", worktreePath: "/demo/loja-online/.worktrees/checkout-v2", status: "ready", lastOpenedAt: ago(3) },
		{ id: "demo-w2", name: "main", branch: "main", projectId: "loja", projectName: "loja-online", projectPath: "/demo/loja-online", projectColor: "emerald", worktreePath: "/demo/loja-online", status: "ready", lastOpenedAt: ago(90) },
		{ id: "demo-w3", name: "fix-login", branch: "fix/login-timeout", projectId: "api", projectName: "api-pagamentos", projectPath: "/demo/api-pagamentos", projectColor: "sky", worktreePath: "/demo/api-pagamentos/.worktrees/fix-login", status: "ready", lastOpenedAt: ago(25) },
		{ id: "demo-w4", name: "docs", branch: "docs/onboarding", projectId: "site", projectName: "site-institucional", projectPath: "/demo/site", projectColor: "violet", worktreePath: "/demo/site", status: "ready", lastOpenedAt: ago(600) },
	];
	const repo = (id: string, name: string, rootPath: string, color: string | null, icon: string | null = null): CatalogRepository => ({
		id,
		projectId: id,
		name,
		displayName: null,
		icon,
		color,
		rootPath,
		baseBranch: "main",
	});
	// The mobile app has no workspace yet: it shows up in Workspaces anyway.
	const repositories = [
		repo("loja", "loja-online", "/demo/loja-online", "emerald"),
		repo("api", "api-pagamentos", "/demo/api-pagamentos", "sky"),
		repo("site", "site-institucional", "/demo/site", "violet"),
		repo("app", "app-mobile", "/demo/app-mobile", "orange", "tucano"),
	];
	const row = (id: string, comb: Comb, title: string, provider: string, minutes: number): DemoSession => ({
		sessionId: id,
		threadTitle: title,
		snippet: null,
		providerId: provider,
		model: null,
		workspaceName: comb.name,
		workspaceBranch: comb.branch,
		workspaceId: comb.id,
		projectId: comb.projectName,
		updatedAt: ago(minutes),
		archivedAt: null,
		events: [],
	});
	const [w1, w2, w3, w4] = combs as [Comb, Comb, Comb, Comb];
	const blocked = row("demo-s1", w1, "Migrar o checkout para o novo gateway", "claude", 2);
	const running = row("demo-s2", w3, "Corrigir timeout no login", "codex", 6);
	const asking = row("demo-s3", w2, "Plano de cache das vitrines", "claude", 30);
	const done = row("demo-s4", w4, "Revisar o guia de onboarding", "claude", 240);
	const failed = row("demo-s5", w1, "Atualizar dependências", "gemini", 1500);

	let seq = 0;
	const events = (session: DemoSession, kinds: Array<RawSessionEvent["kind"]>, startMinutes: number) => {
		seq = 0;
		session.events = kinds.map((kind, index) => ({
			eventId: `${session.sessionId}-${++seq}`,
			sessionId: session.sessionId,
			sequence: seq,
			occurredAt: ago(startMinutes - index * 0.2),
			kind,
		}));
	};
	events(blocked, [
		{ type: "session_started" },
		{ type: "turn_started", turnId: "b1", prompt: "Migre o checkout para o novo gateway de pagamento e rode os testes." },
		{ type: "turn_reasoning_started", turnId: "b1", reasoningId: "b1r", label: "Pensando" },
		{ type: "turn_reasoning_completed", turnId: "b1", reasoningId: "b1r" },
		{ type: "turn_tool_call_started", turnId: "b1", toolCallId: "b1t1", toolName: "Read", toolInput: { file: "src/checkout/gateway.ts" } },
		{ type: "turn_tool_call_completed", turnId: "b1", toolCallId: "b1t1" },
		{ type: "turn_tool_call_started", turnId: "b1", toolCallId: "b1t2", toolName: "Edit", toolInput: { file: "src/checkout/gateway.ts" } },
		{ type: "turn_tool_call_completed", turnId: "b1", toolCallId: "b1t2" },
		{ type: "turn_permission_requested", turnId: "b1", requestId: "demo-p1", question: "Rodar comando\nyarn test checkout", choices: [{ id: "allow", label: "Permitir" }, { id: "deny", label: "Negar" }] },
	], 4);
	events(running, [
		{ type: "session_started" },
		{ type: "turn_started", turnId: "r1", prompt: "O login expira depois de 30 s em redes lentas. Ache a causa e corrija." },
		{ type: "turn_tool_call_started", turnId: "r1", toolCallId: "r1t1", toolName: "Grep", toolInput: { command: "rg timeout src/auth" } },
		{ type: "turn_tool_call_completed", turnId: "r1", toolCallId: "r1t1" },
		{ type: "turn_delta", turnId: "r1", content: "Achei: o cliente HTTP do login usa um timeout fixo de 30 s. " },
		{ type: "turn_tool_call_started", turnId: "r1", toolCallId: "r1t2", toolName: "Bash", toolInput: { command: "yarn test auth" } },
	], 8);
	events(asking, [
		{ type: "session_started" },
		{ type: "turn_started", turnId: "a1", prompt: "Proponha um plano de cache para as vitrines." },
		{ type: "turn_delta", turnId: "a1", content: "Antes de seguir, preciso de uma decisão sua." },
		{
			type: "turn_user_input_requested",
			turnId: "a1",
			requestId: "demo-q1",
			questions: [
				{
					id: "ttl",
					header: "Validade",
					question: "Por quanto tempo uma vitrine pode ficar em cache?",
					options: [
						{ label: "5 minutos", description: "Preço sempre fresco" },
						{ label: "1 hora", description: "Menos carga no banco" },
					],
				},
			],
		},
	], 32);
	events(done, [
		{ type: "session_started" },
		{ type: "turn_started", turnId: "d1", prompt: "Revise o guia de onboarding e corrija links quebrados." },
		{ type: "turn_tool_call_started", turnId: "d1", toolCallId: "d1t1", toolName: "Read", toolInput: { file: "docs/onboarding.md" } },
		{ type: "turn_tool_call_completed", turnId: "d1", toolCallId: "d1t1" },
		{ type: "turn_delta", turnId: "d1", content: "Corrigi 3 links quebrados e atualizei o passo de instalação para Node 22." },
		{ type: "turn_completed", turnId: "d1" },
	], 245);
	events(failed, [
		{ type: "session_started" },
		{ type: "turn_started", turnId: "f1", prompt: "Atualize as dependências menores." },
		{ type: "turn_delta", turnId: "f1", content: "Comecei pela lista de pacotes desatualizados…" },
		{ type: "turn_aborted", turnId: "f1", reason: "user" },
	], 1505);
	return { sessions: [blocked, running, asking, done, failed], combs, repositories };
}
