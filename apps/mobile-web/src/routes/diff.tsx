import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { ArrowLeft, GitBranch, Loader2 } from "lucide-react";
import { apiFetch, ApiError } from "@/lib/api";
import { loadSession, type PairingSession } from "@/lib/session";
import {
	foldEntry,
	type BundleEntry,
	type FileChange,
	type WorktreeDiff,
} from "@/lib/diff";
import { Rest, SectionLabel, Shell } from "@/components/ui";

type Comb = {
	id: string;
	name: string | null;
	projectName: string | null;
	branch: string | null;
};

const CODE_COLOR: Record<FileChange["code"], string> = {
	A: "text-accent",
	M: "text-info",
	D: "text-danger",
	R: "text-wait",
	"?": "text-faint",
};

export function DiffRoute() {
	const { combId } = useParams({ from: "/diff/$combId" });
	const navigate = useNavigate();
	const [session, setSession] = useState<PairingSession | null | undefined>(
		undefined,
	);
	const [diff, setDiff] = useState<WorktreeDiff | null>(null);
	const [comb, setComb] = useState<Comb | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [loading, setLoading] = useState(true);

	useEffect(() => {
		void loadSession().then((s) => setSession(s));
	}, []);

	useEffect(() => {
		if (!session) {
			if (session === null) setLoading(false);
			return;
		}
		let cancelled = false;
		setLoading(true);
		setError(null);
		(async () => {
			try {
				const [bundle, combs] = await Promise.all([
					apiFetch<BundleEntry[]>(session, "/api/v1/diffs/bundle", {
						method: "POST",
						body: JSON.stringify({ combIds: [combId], worktreePaths: [] }),
					}),
					apiFetch<Comb[]>(session, "/api/v1/combs").catch(() => [] as Comb[]),
				]);
				if (cancelled) return;
				const entry = bundle[0];
				setDiff(entry ? foldEntry(entry) : null);
				setComb(combs.find((c) => c.id === combId) ?? null);
			} catch (err) {
				if (cancelled) return;
				setError(
					err instanceof ApiError && err.status === 401
						? "Sessão expirada. Pareie novamente."
						: err instanceof Error
							? err.message
							: "Falha ao carregar o diff.",
				);
			} finally {
				if (!cancelled) setLoading(false);
			}
		})();
		return () => {
			cancelled = true;
		};
	}, [session, combId]);

	const title = comb?.name ?? comb?.projectName ?? "Workspace";
	const branch = diff?.branch ?? comb?.branch ?? null;

	return (
		<Shell>
			<header className="flex items-center gap-2 pb-4">
				<button
					type="button"
					onClick={() => void navigate({ to: "/" })}
					className="-ml-2 rounded-lg p-2 text-mute hover:text-foreground"
					aria-label="Voltar"
				>
					<ArrowLeft className="size-4" />
				</button>
				<div className="min-w-0 flex-1">
					<h1 className="truncate text-[16px] font-semibold leading-tight">
						{title}
					</h1>
					{branch ? (
						<p className="flex items-center gap-1 truncate font-mono text-[10px] text-mute">
							<GitBranch className="size-3 text-faint" />
							{branch}
						</p>
					) : null}
				</div>
			</header>

			{loading ? (
				<div className="flex justify-center py-16 text-mute">
					<Loader2 className="size-5 animate-spin" />
				</div>
			) : session === null ? (
				<Rest title="Sem sessão">
					Pareie o celular primeiro.{" "}
					<Link to="/" className="text-foreground underline">
						Voltar
					</Link>
				</Rest>
			) : error ? (
				<Rest title="Não foi possível ler o diff">{error}</Rest>
			) : !diff || diff.error ? (
				<Rest title="Worktree indisponível">
					{diff?.error ?? "Nenhum dado de diff retornado."}
				</Rest>
			) : diff.clean ? (
				<Rest title="Árvore limpa">
					Nada mudou neste worktree desde o último commit.
				</Rest>
			) : (
				<DiffBody diff={diff} session={session!} workspaceId={combId} />
			)}
		</Shell>
	);
}

function DiffBody({
	diff,
	session,
	workspaceId,
}: {
	diff: WorktreeDiff;
	session: PairingSession;
	workspaceId: string;
}) {
	return (
		<div>
			<StatBar
				files={diff.files.length}
				insertions={diff.insertions}
				deletions={diff.deletions}
			/>
			<div className="mt-5">
				<SectionLabel count={diff.files.length}>Arquivos</SectionLabel>
				<ul className="overflow-hidden rounded-xl border border-border bg-panel">
					{diff.files.map((file) => (
						<PatchFile
							key={file.path}
							file={file}
							session={session}
							workspaceId={workspaceId}
						/>
					))}
				</ul>
			</div>
		</div>
	);
}

function StatBar({
	files,
	insertions,
	deletions,
}: {
	files: number;
	insertions: number;
	deletions: number;
}) {
	const total = insertions + deletions || 1;
	const insPct = Math.round((insertions / total) * 100);
	return (
		<div className="rounded-xl border border-border bg-panel p-3.5">
			<div className="flex items-baseline justify-between font-mono text-[12px]">
				<span className="text-mute">
					{files} arquivo{files === 1 ? "" : "s"}
				</span>
				<span className="flex items-center gap-2 tabular-nums">
					<span className="text-accent">+{insertions}</span>
					<span className="text-danger">−{deletions}</span>
				</span>
			</div>
			{insertions + deletions > 0 ? (
				<div className="mt-2 flex h-1.5 overflow-hidden rounded-full bg-bg">
					<span className="h-full bg-accent" style={{ width: `${insPct}%` }} />
					<span
						className="h-full bg-danger"
						style={{ width: `${100 - insPct}%` }}
					/>
				</div>
			) : null}
			<p className="mt-2 font-mono text-[10px] text-faint">
				+/− de mudanças não commitadas (git diff --stat)
			</p>
		</div>
	);
}

function FilePath({ path }: { path: string }) {
	const slash = path.lastIndexOf("/");
	const dir = slash === -1 ? "" : path.slice(0, slash + 1);
	const base = slash === -1 ? path : path.slice(slash + 1);
	return (
		<div className="flex min-w-0 flex-1 items-baseline font-mono text-[12px]">
			{dir ? <span className="truncate text-faint">{dir}</span> : null}
			<span className="shrink-0 font-medium text-foreground">{base}</span>
		</div>
	);
}

function PatchFile({
	file,
	session,
	workspaceId,
}: {
	file: FileChange;
	session: PairingSession;
	workspaceId: string;
}) {
	const [open, setOpen] = useState(false);
	const [patch, setPatch] = useState<{
		patch: string;
		truncated: boolean;
	} | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [attempt, setAttempt] = useState(0);
	useEffect(() => {
		if (!open) return;
		const controller = new AbortController();
		setError(null);
		setPatch(null);
		void apiFetch<{ patch: string; truncated: boolean }>(
			session,
			`/api/v1/mobile/workspaces/${encodeURIComponent(workspaceId)}/patch?path=${encodeURIComponent(file.path)}`,
			{ signal: controller.signal },
		)
			.then(setPatch)
			.catch((e) => {
				if (!controller.signal.aborted) setError(e.message);
			});
		return () => controller.abort();
	}, [open, session, workspaceId, file.path, attempt]);
	return (
		<li className="border-b border-border/60 last:border-b-0">
			<button
				type="button"
				aria-expanded={open}
				onClick={() => setOpen(!open)}
				className="flex min-h-12 w-full items-center gap-3 px-3.5 py-3 text-left"
			>
				<span
					className={`font-mono text-xs font-bold ${CODE_COLOR[file.code]}`}
				>
					{file.code}
				</span>
				<FilePath path={file.path} />
				<span className="ml-auto text-mute">{open ? "−" : "+"}</span>
			</button>
			{open && (
				<div className="border-t border-border bg-bg">
					{error ? (
						<div role="alert" className="p-3 text-xs text-danger">
							{error}
							<button
								className="ml-2 underline"
								onClick={() => setAttempt((a) => a + 1)}
							>
								Tentar novamente
							</button>
						</div>
					) : !patch ? (
						<p className="p-4 text-xs text-mute">Carregando alterações…</p>
					) : (
						<>
							{patch.truncated && (
								<p className="p-3 text-xs text-wait">
									Arquivo grande: mostrando os primeiros 500 KB.
								</p>
							)}
							{patch.patch ? (
								<pre
									tabIndex={0}
									aria-label={`Alterações em ${file.path}`}
									className="max-h-[60dvh] overflow-auto p-3 font-mono text-[11px] leading-5"
								>
									{patch.patch.split("\n").map((line, i) => (
										<span
											key={i}
											className={`block min-h-5 ${line.startsWith("+") ? "bg-accent/5 text-accent" : line.startsWith("-") ? "bg-danger/5 text-danger" : line.startsWith("@@") ? "text-info" : "text-mute"}`}
										>
											{line || " "}
										</span>
									))}
								</pre>
							) : (
								<p className="p-4 text-xs text-mute">
									Sem alterações de conteúdo neste arquivo.
								</p>
							)}
						</>
					)}
				</div>
			)}
		</li>
	);
}
