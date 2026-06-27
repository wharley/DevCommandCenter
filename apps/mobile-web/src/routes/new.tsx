import { useEffect, useState } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import { AlertTriangle, ArrowLeft, Check, FolderGit2, Loader2 } from "lucide-react";
import { ProviderIcon } from "@/components/provider-icon";
import { ApiError, apiFetch } from "@/lib/api";
import { cn } from "@/lib/cn";
import { loadSession, type PairingSession } from "@/lib/session";

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

type ProviderChoice = {
	id: string;
	label: string;
	description: string;
};

const PROVIDERS: ProviderChoice[] = [
	{
		id: "claude_code",
		label: "Claude Code",
		description: "Anthropic — Sonnet 4.6 por padrão",
	},
	{
		id: "codex",
		label: "Codex",
		description: "OpenAI — GPT-5.4 por padrão",
	},
];

type StartThreadOutput = {
	session?: { id?: string; sessionId?: string };
	thread?: { id?: string };
};

export function NewThreadRoute() {
	const navigate = useNavigate();
	const [session, setSession] = useState<PairingSession | null | undefined>(undefined);
	const [combs, setCombs] = useState<Comb[] | null>(null);
	const [loadError, setLoadError] = useState<string | null>(null);
	const [selectedComb, setSelectedComb] = useState<string | null>(null);
	const [selectedProvider, setSelectedProvider] = useState<string>(PROVIDERS[0]!.id);
	const [submitting, setSubmitting] = useState(false);
	const [submitError, setSubmitError] = useState<string | null>(null);

	useEffect(() => {
		void loadSession().then((s) => setSession(s));
	}, []);

	useEffect(() => {
		if (!session) return;
		setLoadError(null);
		apiFetch<Comb[]>(session, "/api/v1/combs")
			.then((result) => {
				setCombs(result);
				// Pre-select the most recently opened workspace.
				const sorted = [...result].sort((a, b) => {
					const av = a.lastOpenedAt ?? "";
					const bv = b.lastOpenedAt ?? "";
					return bv.localeCompare(av);
				});
				if (sorted[0]) setSelectedComb(sorted[0].id);
			})
			.catch((err) => {
				setLoadError(
					err instanceof ApiError && err.status === 401
						? "Sessão expirada. Pareie novamente."
						: err instanceof Error
							? err.message
							: "Falha ao carregar workspaces.",
				);
			});
	}, [session]);

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

	const submit = async () => {
		if (!selectedComb || submitting) return;
		const comb = combs?.find((c) => c.id === selectedComb);
		if (!comb || !comb.projectId) {
			setSubmitError("Workspace inválido.");
			return;
		}
		setSubmitting(true);
		setSubmitError(null);
		try {
			const result = await apiFetch<StartThreadOutput>(
				session,
				"/api/v1/sessions/start",
				{
					method: "POST",
					body: JSON.stringify({
						workspaceId: comb.id,
						projectId: comb.projectId,
						providerId: selectedProvider,
					}),
				},
			);
			const sessionId =
				result?.session?.sessionId ?? result?.session?.id ?? result?.thread?.id;
			if (!sessionId) {
				throw new Error("Resposta inesperada — sessionId ausente.");
			}
			void navigate({
				to: "/threads/$threadId",
				params: { threadId: sessionId },
				replace: true,
			});
		} catch (err) {
			setSubmitError(err instanceof Error ? err.message : "Falha ao criar.");
		} finally {
			setSubmitting(false);
		}
	};

	const submittable = Boolean(selectedComb) && !submitting;

	return (
		<Shell>
			<header className="flex items-center gap-2 pb-5">
				<Link
					to="/"
					className="-ml-2 rounded-lg p-2 text-mute hover:text-foreground"
				>
					<ArrowLeft className="size-4" />
				</Link>
				<h1 className="text-xl font-semibold">Nova thread</h1>
			</header>

			<section>
				<h2 className="pb-2 text-[11px] font-medium uppercase tracking-wider text-mute">
					Workspace
				</h2>
				{loadError ? (
					<div className="rounded-2xl border border-danger/30 bg-danger/5 p-3 text-[12px] text-danger">
						<AlertTriangle className="mr-1 inline size-3.5" />
						{loadError}
					</div>
				) : combs === null ? (
					<div className="rounded-2xl border border-dashed border-border/70 p-6 text-center text-[12px] text-mute">
						Carregando…
					</div>
				) : combs.length === 0 ? (
					<div className="rounded-2xl border border-dashed border-border/70 p-6 text-center text-[12px] text-mute">
						Nenhum workspace. Crie um pelo desktop primeiro.
					</div>
				) : (
					<ul className="space-y-2">
						{combs.map((comb) => (
							<li key={comb.id}>
								<button
									type="button"
									onClick={() => setSelectedComb(comb.id)}
									className={cn(
										"flex w-full items-start gap-3 rounded-2xl border bg-panel px-4 py-3 text-left transition-colors",
										selectedComb === comb.id
											? "border-accent bg-accent/10"
											: "border-border active:bg-muted/20",
									)}
								>
									<FolderGit2
										className={cn(
											"mt-0.5 size-4 shrink-0",
											selectedComb === comb.id ? "text-accent" : "text-mute",
										)}
									/>
									<div className="min-w-0 flex-1">
										<p className="truncate text-[14px] font-medium">
											{comb.name ?? comb.projectName ?? comb.id}
										</p>
										<p className="mt-0.5 truncate text-[11px] text-mute">
											{[comb.projectName, comb.branch].filter(Boolean).join(" · ")}
										</p>
									</div>
									{selectedComb === comb.id ? (
										<Check className="size-4 shrink-0 text-accent" />
									) : null}
								</button>
							</li>
						))}
					</ul>
				)}
			</section>

			<section className="mt-6">
				<h2 className="pb-2 text-[11px] font-medium uppercase tracking-wider text-mute">
					Agent
				</h2>
				<div className="space-y-2">
					{PROVIDERS.map((p) => (
						<button
							key={p.id}
							type="button"
							onClick={() => setSelectedProvider(p.id)}
							className={cn(
								"flex w-full items-center gap-3 rounded-2xl border bg-panel px-4 py-3 text-left transition-colors",
								selectedProvider === p.id
									? "border-accent bg-accent/10"
									: "border-border active:bg-muted/20",
							)}
						>
							<ProviderIcon provider={p.id} className="size-5 shrink-0" />
							<div className="min-w-0 flex-1">
								<p className="text-[14px] font-medium">{p.label}</p>
								<p className="mt-0.5 text-[11px] text-mute">{p.description}</p>
							</div>
							{selectedProvider === p.id ? (
								<Check className="size-4 shrink-0 text-accent" />
							) : null}
						</button>
					))}
				</div>
			</section>

			{submitError ? (
				<p className="mt-4 rounded-2xl border border-danger/30 bg-danger/5 p-3 text-[12px] text-danger">
					<AlertTriangle className="mr-1 inline size-3.5" />
					{submitError}
				</p>
			) : null}

			<button
				type="button"
				onClick={() => void submit()}
				disabled={!submittable}
				className={cn(
					"mt-6 w-full rounded-2xl bg-accent px-4 py-3.5 text-[15px] font-semibold text-[#04231b] transition-opacity",
					!submittable && "opacity-40",
				)}
			>
				{submitting ? (
					<span className="inline-flex items-center gap-2">
						<Loader2 className="size-4 animate-spin" />
						Criando…
					</span>
				) : (
					"Criar thread"
				)}
			</button>
		</Shell>
	);
}

function Shell({ children }: { children: React.ReactNode }) {
	return (
		<main className="mx-auto flex min-h-dvh max-w-md flex-col px-5 py-8">{children}</main>
	);
}
