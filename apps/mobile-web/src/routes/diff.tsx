import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { ArrowLeft, GitBranch, Loader2 } from "lucide-react";
import { apiFetch, ApiError } from "@/lib/api";
import { cn } from "@/lib/cn";
import { loadSession, type PairingSession } from "@/lib/session";
import { foldEntry, type BundleEntry, type FileChange, type WorktreeDiff } from "@/lib/diff";
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
	const [session, setSession] = useState<PairingSession | null | undefined>(undefined);
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
					<h1 className="truncate text-[16px] font-semibold leading-tight">{title}</h1>
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
				<Rest title="Árvore limpa">Nada mudou neste worktree desde o último commit.</Rest>
			) : (
				<DiffBody diff={diff} />
			)}
		</Shell>
	);
}

function DiffBody({ diff }: { diff: WorktreeDiff }) {
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
					{diff.files.map((file, i) => (
						<li
							key={`${file.path}-${i}`}
							className={cn(
								"flex items-center gap-3 px-3.5 py-2.5",
								i > 0 && "border-t border-border/60",
							)}
						>
							<span
								className={cn(
									"grid size-5 shrink-0 place-items-center rounded font-mono text-[11px] font-bold",
									CODE_COLOR[file.code],
								)}
								title={file.label}
							>
								{file.code}
							</span>
							<FilePath path={file.path} />
						</li>
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
					<span className="h-full bg-danger" style={{ width: `${100 - insPct}%` }} />
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
