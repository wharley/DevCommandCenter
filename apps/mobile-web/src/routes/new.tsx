import { useEffect, useRef, useState } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import { ArrowLeft, ArrowRight, Loader2, RefreshCw } from "lucide-react";
import { Shell, Rest } from "@/components/ui";
import { ProviderIcon } from "@/components/provider-icon";
import { apiFetch } from "@/lib/api";
import { loadSession, type PairingSession } from "@/lib/session";
import { readLocal, writeLocal, removeLocal } from "@/lib/local-data";
import {
	createMobileTask,
	pendingTask,
	forgetTask,
	newRequestId,
	type MobileCatalog,
	type TaskInput,
	type TaskResult,
} from "@/lib/task-api";

type Draft = {
	title: string;
	prompt: string;
	mode: "existing" | "isolated";
	target: string;
	provider: string;
	model: string;
	plan: boolean;
};
const EMPTY: Draft = {
	title: "",
	prompt: "",
	mode: "isolated",
	target: "",
	provider: "",
	model: "",
	plan: false,
};
const field =
	"mt-2 w-full rounded-xl border border-border bg-panel px-3 py-3 text-base outline-none focus:border-accent disabled:opacity-50";

export function NewThreadRoute() {
	const navigate = useNavigate();
	const [session, setSession] = useState<PairingSession | null>();
	const [catalog, setCatalog] = useState<MobileCatalog | null>(null);
	const [draft, setDraft] = useState<Draft>(EMPTY);
	const [error, setError] = useState<string | null>(null);
	const [loading, setLoading] = useState(false);
	const [busy, setBusy] = useState(false);
	const [pending, setPending] = useState<TaskInput | null>(null);
	const [result, setResult] = useState<TaskResult | null>(null);
	const request = useRef<AbortController | null>(null);
	useEffect(() => {
		let disposed = false;
		void loadSession().then((s) => {
			if (disposed) return;
			setSession(s);
			if (s) {
				setDraft(readLocal<Draft>(s, "task-draft") ?? EMPTY);
				setPending(pendingTask(s));
			}
		});
		return () => {
			disposed = true;
			request.current?.abort();
		};
	}, []);

	const refresh = async (active: PairingSession) => {
		setLoading(true);
		setError(null);
		try {
			const data = await apiFetch<MobileCatalog>(
				active,
				"/api/v1/mobile/catalog",
			);
			setCatalog(data);
			setDraft((d) => {
				const mode = data.repositories.length ? d.mode : "existing";
				const targets =
					mode === "isolated"
						? data.repositories
						: data.workspaces.filter((w) => w.state === "ready");
				const providers = data.providers.filter(available);
				return {
					...d,
					mode,
					target: targets.some((t) => t.id === d.target)
						? d.target
						: (targets[0]?.id ?? ""),
					provider: providers.some((p) => p.id === d.provider)
						? d.provider
						: (providers[0]?.id ?? ""),
				};
			});
		} catch (e) {
			setError(e instanceof Error ? e.message : "Falha ao carregar opções.");
		} finally {
			setLoading(false);
		}
	};
	useEffect(() => {
		if (session) void refresh(session);
	}, [session]);
	const change = (patch: Partial<Draft>) => {
		const next = { ...draft, ...patch };
		setDraft(next);
		if (session) writeLocal(session, "task-draft", next);
	};
	const provider = catalog?.providers.find((p) => p.id === draft.provider);
	const targets =
		draft.mode === "isolated"
			? (catalog?.repositories ?? [])
			: (catalog?.workspaces.filter((w) => w.state === "ready") ?? []);
	const valid = Boolean(
		draft.prompt.trim() &&
		targets.some((t) => t.id === draft.target) &&
		provider &&
		available(provider),
	);

	const submit = async () => {
		if (!session || request.current || (!pending && !valid)) return;
		const input: TaskInput = pending ?? {
			requestId: newRequestId(),
			workspaceId: draft.mode === "existing" ? draft.target : null,
			repositoryId: draft.mode === "isolated" ? draft.target : null,
			title:
				draft.title.trim() || draft.prompt.trim().split("\n")[0]!.slice(0, 100),
			prompt: draft.prompt,
			providerId: draft.provider,
			model: draft.model || null,
			planMode: draft.plan,
		};
		setPending(input);
		setBusy(true);
		setError(null);
		const controller = new AbortController();
		request.current = controller;
		try {
			const outcome = await createMobileTask(session, input, controller.signal);
			if (controller.signal.aborted) return;
			setResult(outcome);
			if (outcome.state === "started" && outcome.sessionId) {
				forgetTask(session);
				removeLocal(session, "task-draft");
				void navigate({
					to: "/threads/$threadId",
					params: { threadId: outcome.sessionId },
					replace: true,
				});
			} else if (outcome.state === "failed")
				setError(outcome.error ?? "Não foi possível iniciar o agente.");
			else
				setError(
					"A criação continua no computador. Toque em verificar para recuperar o resultado.",
				);
		} catch (e) {
			if (!controller.signal.aborted)
				setError(e instanceof Error ? e.message : "Falha ao criar tarefa.");
		} finally {
			request.current = null;
			if (!controller.signal.aborted) setBusy(false);
		}
	};

	return (
		<Shell>
			<header className="mb-6 flex items-center gap-3">
				<Link to="/" aria-label="Voltar" className="rounded-xl p-3 text-mute">
					<ArrowLeft className="size-5" />
				</Link>
				<div>
					<p className="font-mono text-[10px] uppercase tracking-widest text-accent">
						Do celular ao código
					</p>
					<h1 className="text-xl font-semibold">Nova tarefa</h1>
				</div>
			</header>
			{session === undefined ? (
				<Rest title="Carregando…" />
			) : !session ? (
				<Rest title="Pareie seu celular">
					<Link to="/">Voltar ao início</Link>
				</Rest>
			) : (
				<>
					{pending ? (
						<section className="mb-5 rounded-2xl border border-wait/40 bg-wait/5 p-4 text-sm">
							<p className="font-medium">
								{busy
									? "Preparando sua tarefa…"
									: "Há uma criação para acompanhar"}
							</p>
							<p className="mt-2 text-mute">
								{pending.title}. O pedido fica salvo para você recuperar o
								resultado se a conexão cair.
							</p>
							{result?.sessionId && (
								<Link
									className="mt-3 inline-block text-accent underline"
									to="/threads/$threadId"
									params={{ threadId: result.sessionId }}
								>
									Abrir conversa criada
								</Link>
							)}
							{result?.state === "failed" && (
								<button
									type="button"
									className="mt-3 block underline"
									onClick={() => {
										forgetTask(session);
										setPending(null);
										setResult(null);
										setError(null);
									}}
								>
									Preparar outra tarefa
								</button>
							)}
						</section>
					) : null}
					<fieldset
						disabled={busy || Boolean(pending)}
						className="space-y-5 disabled:opacity-60"
					>
						<label className="block text-sm font-medium">
							O que vamos fazer?
							<textarea
								className={`${field} min-h-36 resize-y`}
								placeholder="Descreva a mudança, o problema ou a ideia…"
								value={draft.prompt}
								maxLength={100000}
								onChange={(e) => change({ prompt: e.target.value })}
							/>
							<span className="mt-1 block text-xs font-normal text-mute">
								Rascunho salvo neste celular. Enviado somente quando você
								iniciar.
							</span>
						</label>
						<label className="block text-sm font-medium">
							Título <span className="font-normal text-mute">· opcional</span>
							<input
								className={field}
								value={draft.title}
								maxLength={200}
								placeholder="Um nome para encontrar depois"
								onChange={(e) => change({ title: e.target.value })}
							/>
						</label>
						<div>
							<p className="text-sm font-medium">Onde trabalhar</p>
							<div className="mt-2 grid grid-cols-2 gap-2">
								{(["isolated", "existing"] as const).map((mode) => (
									<button
										key={mode}
										type="button"
										aria-pressed={draft.mode === mode}
										className={`rounded-xl border p-3 text-sm ${draft.mode === mode ? "border-accent bg-accent/10" : "border-border bg-panel"}`}
										onClick={() => change({ mode, target: "" })}
									>
										{mode === "isolated"
											? "Nova área isolada"
											: "Área existente"}
									</button>
								))}
							</div>
							<p className="mt-2 text-xs leading-relaxed text-mute">
								{draft.mode === "isolated"
									? "Cria uma branch e worktree a partir da base do projeto. Alterações locais não salvas em commits ficam na área original."
									: "Continua no workspace escolhido, usando os arquivos que já estão nele."}
							</p>
							<label className="block">
								<span className="sr-only">
									{draft.mode === "isolated" ? "Projeto" : "Workspace"}
								</span>
								<select
									className={field}
									value={draft.target}
									onChange={(e) => change({ target: e.target.value })}
								>
									<option value="">
										{loading
											? "Carregando…"
											: "Escolha um projeto ou workspace"}
									</option>
									{targets.map((t) => (
										<option key={t.id} value={t.id}>
											{t.name || t.rootPath.split("/").pop()} · {t.baseBranch}
										</option>
									))}
								</select>
							</label>
							{catalog && targets.length === 0 && (
								<p className="mt-2 text-xs text-wait">
									Nenhuma opção disponível. Cadastre um repositório no DCC ou
									escolha outra forma de trabalhar.
								</p>
							)}
						</div>
						<div>
							<p className="text-sm font-medium">Agente</p>
							<div className="mt-2 grid grid-cols-2 gap-2">
								{catalog?.providers.map((p) => (
									<button
										type="button"
										key={p.id}
										disabled={!available(p)}
										aria-pressed={draft.provider === p.id}
										onClick={() => change({ provider: p.id, model: "" })}
										className={`flex items-center gap-2 rounded-xl border p-3 text-left text-sm disabled:opacity-40 ${draft.provider === p.id ? "border-accent bg-accent/10" : "border-border bg-panel"}`}
									>
										<ProviderIcon provider={p.id} className="size-5 shrink-0" />
										<span>
											{p.label}
											{!available(p) && (
												<small className="block text-mute">
													Indisponível no host
												</small>
											)}
										</span>
									</button>
								))}
							</div>
						</div>
						<label className="block text-sm font-medium">
							Modelo
							<select
								className={field}
								value={draft.model}
								onChange={(e) => change({ model: e.target.value })}
							>
								<option value="">Padrão do agente</option>
								{provider?.models.map((m) => (
									<option key={m.id} value={m.id}>
										{m.label}
										{m.recommended ? " · recomendado" : ""}
									</option>
								))}
							</select>
						</label>
						<label className="flex items-center gap-3 rounded-xl border border-border bg-panel p-4 text-sm">
							<input
								type="checkbox"
								className="size-5 accent-[var(--color-accent)]"
								checked={draft.plan}
								onChange={(e) => change({ plan: e.target.checked })}
							/>
							<span>Planejar antes de implementar</span>
						</label>
					</fieldset>
					{error && (
						<p
							role="alert"
							className="mt-4 rounded-xl border border-danger/30 bg-danger/5 p-4 text-sm text-danger"
						>
							{error}
						</p>
					)}
					{!pending && (
						<button
							type="button"
							disabled={loading}
							onClick={() => void refresh(session)}
							className="mt-3 flex items-center justify-center gap-2 p-3 text-xs text-mute"
						>
							<RefreshCw
								className={`size-4 ${loading ? "animate-spin" : ""}`}
							/>
							Atualizar agentes e projetos
						</button>
					)}
					<button
						type="button"
						disabled={
							busy || (!pending && !valid) || result?.state === "failed"
						}
						onClick={() => void submit()}
						className="mt-5 flex min-h-12 items-center justify-center gap-2 rounded-2xl bg-accent p-4 font-semibold text-accent-ink disabled:opacity-40"
					>
						{busy ? (
							<Loader2 className="size-5 animate-spin" />
						) : (
							<ArrowRight className="size-5" />
						)}
						{busy
							? "Criando e iniciando…"
							: pending
								? "Verificar criação"
								: "Iniciar tarefa"}
					</button>
				</>
			)}
		</Shell>
	);
}
function available(p: MobileCatalog["providers"][number]): boolean {
	return p.enabled && !(typeof p.health === "object" && p.health.Unhealthy);
}
