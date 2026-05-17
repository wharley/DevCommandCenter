import { useEffect, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import {
	Activity,
	Cpu,
	Loader2,
	LogOut,
	RefreshCw,
	Smartphone,
} from "lucide-react";
import { ApiError, apiFetch } from "@/lib/api";
import { cn } from "@/lib/cn";
import { clearSession, loadSession, type PairingSession } from "@/lib/session";

type DaemonStatus = {
	running: boolean;
	mode: string;
	pid: number | null;
	cpuPercent: number;
	memoryMb: number;
	totalTasks: number;
	enabledTasks: number;
	runningTasks: number;
	startedAt: string | null;
	lastTickAt: string | null;
};

type Task = {
	taskId: string;
	taskName: string;
	projectName: string;
	projectId: string;
	status: "idle" | "running" | "disabled" | string;
	enabled: boolean;
	schedule: string | null;
	command: string;
	lastRunAt: string | null;
	nextRunAt: string | null;
};

type Bootstrap =
	| { state: "loading" }
	| { state: "unpaired" }
	| { state: "ready"; session: PairingSession };

export function HomeRoute() {
	const navigate = useNavigate();
	const [boot, setBoot] = useState<Bootstrap>({ state: "loading" });

	useEffect(() => {
		void loadSession().then((s) => {
			if (!s) {
				setBoot({ state: "unpaired" });
				return;
			}
			setBoot({ state: "ready", session: s });
		});
	}, []);

	useEffect(() => {
		if (boot.state === "unpaired") {
			// No saved session — drop the user on a friendlier landing rather
			// than the bare /pair (which only makes sense from a QR scan).
		}
	}, [boot.state]);

	if (boot.state === "loading") {
		return (
			<Shell>
				<div className="flex h-[50vh] items-center justify-center text-mute">
					<Loader2 className="size-5 animate-spin" />
				</div>
			</Shell>
		);
	}

	if (boot.state === "unpaired") {
		return <UnpairedView />;
	}

	return (
		<PairedHome
			session={boot.session}
			onLogout={async () => {
				await clearSession();
				setBoot({ state: "unpaired" });
				void navigate({ to: "/", replace: true });
			}}
		/>
	);
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
					Abra o app desktop, vá em Settings &rarr; Dispositivos pareados &rarr;
					Parear novo dispositivo. Escaneie o QR com este celular.
				</p>
			</section>
		</Shell>
	);
}

function PairedHome({
	session,
	onLogout,
}: {
	session: PairingSession;
	onLogout: () => Promise<void>;
}) {
	const [status, setStatus] = useState<DaemonStatus | null>(null);
	const [tasks, setTasks] = useState<Task[] | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [refreshing, setRefreshing] = useState(false);

	const refresh = async () => {
		setRefreshing(true);
		setError(null);
		try {
			const [s, t] = await Promise.all([
				apiFetch<DaemonStatus>(session, "/api/v1/status"),
				apiFetch<Task[]>(session, "/api/v1/tasks"),
			]);
			setStatus(s);
			setTasks(t);
		} catch (err) {
			if (err instanceof ApiError && err.status === 401) {
				setError("Sessão expirada. Pareie novamente no desktop.");
			} else {
				setError(err instanceof Error ? err.message : "Falha ao carregar dados.");
			}
		} finally {
			setRefreshing(false);
		}
	};

	useEffect(() => {
		void refresh();
		const id = window.setInterval(refresh, 10_000);
		return () => window.clearInterval(id);
	}, []);

	return (
		<Shell>
			<header className="flex items-center justify-between gap-3 px-1 pb-5">
				<div>
					<h1 className="text-xl font-semibold">Dev Command Center</h1>
					<p className="mt-0.5 break-all font-mono text-[11px] text-mute/80">
						{session.backendUrl}
					</p>
				</div>
				<button
					type="button"
					onClick={onLogout}
					className="-mr-1 rounded-lg p-2 text-mute hover:text-foreground"
					title="Desconectar"
				>
					<LogOut className="size-4" />
				</button>
			</header>

			<StatusCard status={status} refreshing={refreshing} onRefresh={() => void refresh()} />

			{error ? (
				<p className="mt-3 rounded-xl border border-danger/30 bg-danger/5 px-4 py-3 text-[12px] text-danger">
					{error}
				</p>
			) : null}

			<section className="mt-5">
				<h2 className="px-1 pb-2 text-[11px] font-medium uppercase tracking-wider text-mute">
					Tasks
				</h2>
				<TasksList tasks={tasks} />
			</section>
		</Shell>
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
						<p className="text-[11px] text-mute">
							{status
								? `${status.runningTasks} de ${status.enabledTasks} task(s) rodando`
								: "Carregando…"}
						</p>
					</div>
				</div>
				<button
					type="button"
					onClick={onRefresh}
					disabled={refreshing}
					className="rounded-lg p-1.5 text-mute hover:text-foreground disabled:opacity-50"
					title="Atualizar"
				>
					<RefreshCw
						className={cn("size-3.5", refreshing && "animate-spin")}
					/>
				</button>
			</div>

			{status ? (
				<div className="mt-3 grid grid-cols-2 gap-2 border-t border-border/60 pt-3">
					<Metric icon={<Cpu className="size-3.5" />} label="CPU" value={`${status.cpuPercent.toFixed(1)}%`} />
					<Metric label="Memória" value={`${status.memoryMb.toFixed(0)} MB`} />
				</div>
			) : null}
		</section>
	);
}

function Metric({
	icon,
	label,
	value,
}: {
	icon?: React.ReactNode;
	label: string;
	value: string;
}) {
	return (
		<div>
			<div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-mute">
				{icon}
				{label}
			</div>
			<div className="mt-0.5 font-mono text-[14px] text-foreground">{value}</div>
		</div>
	);
}

function TasksList({ tasks }: { tasks: Task[] | null }) {
	if (tasks === null) {
		return (
			<div className="rounded-2xl border border-dashed border-border/70 p-6 text-center text-[12px] text-mute">
				Carregando tasks…
			</div>
		);
	}
	if (tasks.length === 0) {
		return (
			<div className="rounded-2xl border border-dashed border-border/70 p-6 text-center text-[12px] text-mute">
				Nenhuma task configurada.
			</div>
		);
	}
	return (
		<ul className="space-y-2">
			{tasks.map((task) => (
				<li
					key={task.taskId}
					className="rounded-2xl border border-border bg-panel px-4 py-3"
				>
					<div className="flex items-start justify-between gap-3">
						<div className="min-w-0">
							<p className="truncate text-[14px] font-medium">{task.taskName}</p>
							<p className="mt-0.5 truncate text-[11px] text-mute">
								{task.projectName} · {task.command}
							</p>
						</div>
						<TaskBadge status={task.status} enabled={task.enabled} />
					</div>
					{task.schedule ? (
						<p className="mt-2 font-mono text-[10px] text-mute/80">
							cron: {task.schedule}
						</p>
					) : null}
				</li>
			))}
		</ul>
	);
}

function TaskBadge({ status, enabled }: { status: string; enabled: boolean }) {
	const palette = (() => {
		if (!enabled) return "border-border bg-bg text-mute";
		if (status === "running") return "border-accent/30 bg-accent/10 text-accent";
		if (status === "idle") return "border-border bg-panel text-foreground";
		return "border-border bg-panel text-mute";
	})();
	return (
		<span
			className={cn(
				"shrink-0 rounded-full border px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider",
				palette,
			)}
		>
			{enabled ? status : "off"}
		</span>
	);
}

function Shell({ children }: { children: React.ReactNode }) {
	return (
		<main className="mx-auto flex min-h-dvh max-w-md flex-col px-5 py-8">{children}</main>
	);
}
