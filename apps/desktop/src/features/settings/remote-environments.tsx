import { useEffect, useMemo, useState } from "react";
import {
	CheckCircle2,
	Copy,
	Loader2,
	Play,
	RefreshCw,
	Square,
	Trash2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
	daemonGetStatus,
	daemonHealth,
	getCurrentBackendTarget,
} from "@/lib/daemon-api";
import {
	launchRemoteSshTunnel,
	listRemoteSshTunnels,
	stopRemoteSshTunnel,
	type RemoteTunnelSnapshot,
} from "@/lib/remote-api";
import {
	readActiveRemoteEnvironmentId,
	readRemoteEnvironments,
	writeActiveRemoteEnvironmentId,
	writeRemoteEnvironments,
	type SavedRemoteEnvironment,
} from "./remote-environments-store";

type DraftEnvironment = {
	label: string;
	sshTarget: string;
	remoteCommand: string;
	localPort: string;
	remotePort: string;
};

type RemoteProbe = {
	healthStatus: "ok" | "degraded" | "unknown";
	daemonStatus: string | null;
	statusSummary: string | null;
	errorMessage: string | null;
	checkedAt: string | null;
};

function defaultDraft(): DraftEnvironment {
	return {
		label: "",
		sshTarget: "",
		remoteCommand: "dccd-http",
		localPort: "",
		remotePort: "9876",
	};
}

function createEnvironmentId() {
	if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
		return crypto.randomUUID();
	}
	return `remote-${Date.now()}`;
}

function parsePort(value: string): number | null {
	const trimmed = value.trim();
	if (!trimmed) {
		return null;
	}
	const port = Number.parseInt(trimmed, 10);
	if (!Number.isFinite(port) || port < 1 || port > 65535) {
		return null;
	}
	return port;
}

export function RemoteEnvironmentsPanel() {
	const { t } = useTranslation("common");
	const [draft, setDraft] = useState<DraftEnvironment>(() => defaultDraft());
	const [environments, setEnvironments] = useState<SavedRemoteEnvironment[]>(() =>
		readRemoteEnvironments(),
	);
	const [tunnels, setTunnels] = useState<Record<string, RemoteTunnelSnapshot>>({});
	const [activeEnvironmentId, setActiveEnvironmentId] = useState<string | null>(() =>
		readActiveRemoteEnvironmentId(),
	);
	const [probes, setProbes] = useState<Record<string, RemoteProbe>>({});
	const [busyId, setBusyId] = useState<string | null>(null);
	const [isRefreshing, setIsRefreshing] = useState(false);
	const [localBackendProbe, setLocalBackendProbe] = useState<RemoteProbe | null>(null);

	const persistEnvironments = (next: SavedRemoteEnvironment[]) => {
		setEnvironments(next);
		writeRemoteEnvironments(next);
	};

	const setActiveEnvironment = (environmentId: string | null) => {
		setActiveEnvironmentId(environmentId);
		writeActiveRemoteEnvironmentId(environmentId);
	};

	const probeEnvironment = async (environment: SavedRemoteEnvironment) => {
		try {
			const [health, status] = await Promise.all([
				daemonHealth(environment),
				daemonGetStatus(environment),
			]);

			let statusSummary: string | null = null;
			if (status && typeof status === "object") {
				const record = status as Record<string, unknown>;
				const totalRunningPanes =
					typeof record.totalRunningPanes === "number" ? record.totalRunningPanes : null;
				const workingAgents =
					typeof record.workingAgents === "number" ? record.workingAgents : null;
				const waitingAgents =
					typeof record.waitingAgents === "number" ? record.waitingAgents : null;
				if (
					totalRunningPanes !== null ||
					workingAgents !== null ||
					waitingAgents !== null
				) {
					statusSummary = [
						totalRunningPanes !== null ? `${totalRunningPanes} panes` : null,
						workingAgents !== null ? `${workingAgents} working` : null,
						waitingAgents !== null ? `${waitingAgents} waiting` : null,
					]
						.filter(Boolean)
						.join(" · ");
				}
			}

			let healthStatus: "ok" | "degraded" | "unknown" = "unknown";
			let daemonStatus: string | null = null;
			if (health && typeof health === "object") {
				const record = health as Record<string, unknown>;
				if (record.status === "ok") {
					healthStatus = "ok";
				} else if (typeof record.status === "string") {
					healthStatus = "degraded";
				}
				if (typeof record.daemon === "string") {
					daemonStatus = record.daemon;
				}
			}

			setProbes((current) => ({
				...current,
				[environment.id]: {
					healthStatus,
					daemonStatus,
					statusSummary,
					errorMessage: null,
					checkedAt: new Date().toISOString(),
				},
			}));
		} catch (error) {
			setProbes((current) => ({
				...current,
				[environment.id]: {
					healthStatus: "degraded",
					daemonStatus: null,
					statusSummary: null,
					errorMessage: String(error),
					checkedAt: new Date().toISOString(),
				},
			}));
		}
	};

	const refreshTunnels = async () => {
		setIsRefreshing(true);
		try {
			const response = await listRemoteSshTunnels();
			const next = Object.fromEntries(
				response.tunnels.map((tunnel) => [tunnel.environmentId, tunnel]),
			);
			setTunnels(next);
		} catch (error) {
			toast.error(String(error));
		} finally {
			setIsRefreshing(false);
		}
	};

	useEffect(() => {
		void refreshTunnels();
	}, []);

	useEffect(() => {
		const activeEnvironment = environments.find(
			(environment) => environment.id === activeEnvironmentId,
		);
		const target = getCurrentBackendTarget();
		if (target.kind === "local") {
			void (async () => {
				try {
					const [health, status] = await Promise.all([daemonHealth(), daemonGetStatus()]);
					let statusSummary: string | null = null;
					if (status && typeof status === "object") {
						const record = status as Record<string, unknown>;
						const totalRunningPanes =
							typeof record.totalRunningPanes === "number"
								? record.totalRunningPanes
								: null;
						const workingAgents =
							typeof record.workingAgents === "number" ? record.workingAgents : null;
						const waitingAgents =
							typeof record.waitingAgents === "number" ? record.waitingAgents : null;
						statusSummary = [
							totalRunningPanes !== null ? `${totalRunningPanes} panes` : null,
							workingAgents !== null ? `${workingAgents} working` : null,
							waitingAgents !== null ? `${waitingAgents} waiting` : null,
						]
							.filter(Boolean)
							.join(" · ");
					}
					let healthStatus: "ok" | "degraded" | "unknown" = "unknown";
					let daemonStatus: string | null = null;
					if (health && typeof health === "object") {
						const record = health as Record<string, unknown>;
						if (record.status === "ok") {
							healthStatus = "ok";
						} else if (typeof record.status === "string") {
							healthStatus = "degraded";
						}
						if (typeof record.daemon === "string") {
							daemonStatus = record.daemon;
						}
					}
					setLocalBackendProbe({
						healthStatus,
						daemonStatus,
						statusSummary,
						errorMessage: null,
						checkedAt: new Date().toISOString(),
					});
				} catch (error) {
					setLocalBackendProbe({
						healthStatus: "degraded",
						daemonStatus: null,
						statusSummary: null,
						errorMessage: String(error),
						checkedAt: new Date().toISOString(),
					});
				}
			})();
			return;
		}

		setLocalBackendProbe(null);
		if (!activeEnvironment?.endpoint || !activeEnvironment.bearerToken) {
			return;
		}

		void probeEnvironment(activeEnvironment);
		const intervalId = window.setInterval(() => {
			void probeEnvironment(activeEnvironment);
		}, 15000);
		return () => window.clearInterval(intervalId);
	}, [activeEnvironmentId, environments]);

	const activeCount = useMemo(
		() => Object.values(tunnels).filter((tunnel) => tunnel.status === "running").length,
		[tunnels],
	);

	const handleAdd = () => {
		const label = draft.label.trim();
		const sshTarget = draft.sshTarget.trim();
		const remoteCommand = draft.remoteCommand.trim() || "dccd-http";
		const remotePort = parsePort(draft.remotePort);
		const localPort = parsePort(draft.localPort);

		if (!label || !sshTarget || !remotePort) {
			toast.error(t("settings.connections.addValidation"));
			return;
		}

		const next: SavedRemoteEnvironment = {
			id: createEnvironmentId(),
			label,
			sshTarget,
			remoteCommand,
			localPort,
			remotePort,
			bearerToken: null,
			endpoint: null,
			lastStartedAt: null,
			tmuxAvailable: null,
		};
		const updated = [next, ...environments];
		persistEnvironments(updated);
		if (activeEnvironmentId && !updated.some((environment) => environment.id === activeEnvironmentId)) {
			setActiveEnvironment(null);
		}
		setDraft(defaultDraft());
	};

	const handleDelete = async (environment: SavedRemoteEnvironment) => {
		if (tunnels[environment.id]?.status === "running") {
			try {
				await stopRemoteSshTunnel(environment.id);
			} catch {
				// keep deleting local config even if the tunnel already died
			}
		}
		const next = environments.filter((candidate) => candidate.id !== environment.id);
		persistEnvironments(next);
		if (activeEnvironmentId === environment.id) {
			setActiveEnvironment(null);
		}
		setTunnels((current) => {
			const copy = { ...current };
			delete copy[environment.id];
			return copy;
		});
		setProbes((current) => {
			const copy = { ...current };
			delete copy[environment.id];
			return copy;
		});
	};

	const handleConnect = async (environment: SavedRemoteEnvironment) => {
		setBusyId(environment.id);
		try {
			const result = await launchRemoteSshTunnel({
				environmentId: environment.id,
				sshTarget: environment.sshTarget,
				remoteCommand: environment.remoteCommand,
				localPort: environment.localPort,
				remotePort: environment.remotePort,
				bearerToken: environment.bearerToken,
			});
			const tunnel = result.tunnel;
			setTunnels((current) => ({ ...current, [environment.id]: tunnel }));
			const updatedEnvironment: SavedRemoteEnvironment = {
				...environment,
				localPort: tunnel.localPort,
				remotePort: tunnel.remotePort,
				bearerToken: tunnel.bearerToken,
				endpoint: tunnel.endpoint,
				lastStartedAt: tunnel.startedAt,
				tmuxAvailable: tunnel.tmuxAvailable,
			};
			persistEnvironments(
				environments.map((candidate) =>
					candidate.id === environment.id ? updatedEnvironment : candidate,
				),
			);
			void probeEnvironment(updatedEnvironment);
			toast.success(
				t("settings.connections.connectedToast", {
					label: environment.label,
				}),
			);
		} catch (error) {
			toast.error(
				t("settings.connections.connectError", {
					label: environment.label,
				}),
			);
			toast.error(String(error));
		} finally {
			setBusyId(null);
		}
	};

	const handleDisconnect = async (environment: SavedRemoteEnvironment) => {
		setBusyId(environment.id);
		try {
			await stopRemoteSshTunnel(environment.id);
			setTunnels((current) => {
				const copy = { ...current };
				delete copy[environment.id];
				return copy;
			});
		} catch (error) {
			toast.error(String(error));
		} finally {
			setBusyId(null);
		}
	};

	const copyEndpoint = async (environment: SavedRemoteEnvironment) => {
		const endpoint = tunnels[environment.id]?.endpoint ?? environment.endpoint;
		if (!endpoint) {
			toast.error(t("settings.connections.copyMissing"));
			return;
		}
		try {
			await navigator.clipboard.writeText(endpoint);
			toast.success(t("settings.connections.copySuccess"));
		} catch (error) {
			toast.error(String(error));
		}
	};

	return (
		<section className="space-y-4">
			<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
				<div className="flex items-start justify-between gap-4">
					<div className="min-w-0">
						<h3 className="text-[14px] font-medium text-foreground">
							{t("settings.connections.title")}
						</h3>
						<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
							{t("settings.connections.body")}
						</p>
					</div>
					<Badge
						variant={activeCount > 0 ? "success" : "outline"}
						className="h-8 px-3 text-[12px] font-normal"
					>
						{t("settings.connections.activeBadge", { count: activeCount })}
					</Badge>
				</div>
				<p className="mt-3 text-[11px] leading-relaxed text-muted-foreground">
					{t("settings.connections.requirements")}
				</p>
				<div className="mt-3 rounded-lg border border-border/50 bg-background/70 p-3 text-[12px] text-muted-foreground">
					<p>
						<strong className="text-foreground">
							{t("settings.connections.currentBackend")}:
						</strong>{" "}
						{activeEnvironmentId
							? environments.find((environment) => environment.id === activeEnvironmentId)
									?.label ?? t("settings.connections.notConnected")
							: t("settings.connections.localBackend")}
					</p>
					{!activeEnvironmentId && localBackendProbe ? (
						<p className="mt-1">
							<strong className="text-foreground">
								{t("settings.connections.fields.health")}:
							</strong>{" "}
							{localBackendProbe.healthStatus === "ok"
								? t("settings.connections.health.ok")
								: localBackendProbe.healthStatus === "degraded"
									? t("settings.connections.health.degraded")
									: t("settings.connections.health.unknown")}
							{localBackendProbe.statusSummary
								? ` · ${localBackendProbe.statusSummary}`
								: ""}
						</p>
					) : null}
				</div>
			</div>

			<div className="rounded-xl border border-border/60 p-4">
				<div className="grid gap-3 md:grid-cols-2">
					<div className="grid gap-1.5">
						<label className="text-[12px] font-medium text-foreground">
							{t("settings.connections.fields.label")}
						</label>
						<Input
							value={draft.label}
							onChange={(event) =>
								setDraft((current) => ({ ...current, label: event.target.value }))
							}
							placeholder={t("settings.connections.placeholders.label")}
						/>
					</div>
					<div className="grid gap-1.5">
						<label className="text-[12px] font-medium text-foreground">
							{t("settings.connections.fields.sshTarget")}
						</label>
						<Input
							value={draft.sshTarget}
							onChange={(event) =>
								setDraft((current) => ({ ...current, sshTarget: event.target.value }))
							}
							placeholder="user@example.com"
						/>
					</div>
					<div className="grid gap-1.5">
						<label className="text-[12px] font-medium text-foreground">
							{t("settings.connections.fields.remoteCommand")}
						</label>
						<Input
							value={draft.remoteCommand}
							onChange={(event) =>
								setDraft((current) => ({
									...current,
									remoteCommand: event.target.value,
								}))
							}
							placeholder="dccd-http"
						/>
					</div>
					<div className="grid gap-1.5 md:grid-cols-2 md:gap-3">
						<div className="grid gap-1.5">
							<label className="text-[12px] font-medium text-foreground">
								{t("settings.connections.fields.remotePort")}
							</label>
							<Input
								value={draft.remotePort}
								onChange={(event) =>
									setDraft((current) => ({ ...current, remotePort: event.target.value }))
								}
								placeholder="9876"
							/>
						</div>
						<div className="grid gap-1.5">
							<label className="text-[12px] font-medium text-foreground">
								{t("settings.connections.fields.localPort")}
							</label>
							<Input
								value={draft.localPort}
								onChange={(event) =>
									setDraft((current) => ({ ...current, localPort: event.target.value }))
								}
								placeholder={t("settings.connections.placeholders.localPort")}
							/>
						</div>
					</div>
				</div>

				<div className="mt-4 flex flex-wrap gap-2">
					<Button type="button" onClick={handleAdd}>
						{t("settings.connections.add")}
					</Button>
					<Button type="button" variant="outline" onClick={() => void refreshTunnels()}>
						<RefreshCw className={`size-3.5 ${isRefreshing ? "animate-spin" : ""}`} />
						{t("settings.connections.refresh")}
					</Button>
				</div>
			</div>

			<div className="space-y-3">
				{environments.length === 0 ? (
					<div className="rounded-xl border border-dashed border-border/70 p-4 text-[12px] text-muted-foreground">
						{t("settings.connections.empty")}
					</div>
				) : null}

				{environments.map((environment) => {
					const tunnel = tunnels[environment.id] ?? null;
					const probe = probes[environment.id] ?? null;
					const isBusy = busyId === environment.id;
					const isRunning = tunnel?.status === "running";
					const isActive = environment.id === activeEnvironmentId;
					const endpoint = tunnel?.endpoint ?? environment.endpoint;
					const tmuxAvailable = tunnel?.tmuxAvailable ?? environment.tmuxAvailable;
					const terminalPersistenceLabel =
						tmuxAvailable === true
							? t("settings.connections.persistence.tmux")
							: tmuxAvailable === false
								? t("settings.connections.persistence.pty")
								: t("settings.connections.persistence.unknown");

					return (
						<div key={environment.id} className="rounded-xl border border-border/60 p-4">
							<div className="flex flex-wrap items-start justify-between gap-3">
								<div className="min-w-0">
									<div className="flex flex-wrap items-center gap-2">
										<h3 className="text-[14px] font-medium text-foreground">
											{environment.label}
										</h3>
										{isActive ? (
											<Badge variant="secondary">
												{t("settings.connections.status.active")}
											</Badge>
										) : null}
										<Badge variant={isRunning ? "success" : "outline"}>
											{isRunning
												? t("settings.connections.status.running")
												: t("settings.connections.status.idle")}
										</Badge>
										<Badge variant={tmuxAvailable ? "secondary" : "outline"}>
											{terminalPersistenceLabel}
										</Badge>
									</div>
									<p className="mt-1 break-all font-mono text-[11px] text-muted-foreground">
										{environment.sshTarget}
									</p>
								</div>

								<div className="flex flex-wrap gap-2">
									<Button
										type="button"
										variant={isActive ? "default" : "outline"}
										size="sm"
										onClick={() => setActiveEnvironment(isActive ? null : environment.id)}
									>
										<CheckCircle2 className="size-3.5" />
										{isActive
											? t("settings.connections.clearActive")
											: t("settings.connections.setActive")}
									</Button>
									{isRunning ? (
										<Button
											type="button"
											variant="outline"
											size="sm"
											disabled={isBusy}
											onClick={() => void handleDisconnect(environment)}
										>
											{isBusy ? (
												<Loader2 className="size-3.5 animate-spin" />
											) : (
												<Square className="size-3.5" />
											)}
											{t("settings.connections.disconnect")}
										</Button>
									) : (
										<Button
											type="button"
											size="sm"
											disabled={isBusy}
											onClick={() => void handleConnect(environment)}
										>
											{isBusy ? (
												<Loader2 className="size-3.5 animate-spin" />
											) : (
												<Play className="size-3.5" />
											)}
											{t("settings.connections.connect")}
										</Button>
									)}
									<Button
										type="button"
										variant="outline"
										size="sm"
										onClick={() => void copyEndpoint(environment)}
									>
										<Copy className="size-3.5" />
										{t("settings.connections.copy")}
									</Button>
									<Button
										type="button"
										variant="ghost"
										size="sm"
										onClick={() => void handleDelete(environment)}
									>
										<Trash2 className="size-3.5" />
										{t("settings.connections.delete")}
									</Button>
								</div>
							</div>

							<div className="mt-3 grid gap-2 text-[12px] text-muted-foreground md:grid-cols-2">
								<p>
									<strong className="text-foreground">
										{t("settings.connections.fields.remoteCommand")}:
									</strong>{" "}
									<span className="font-mono">{environment.remoteCommand}</span>
								</p>
								<p>
									<strong className="text-foreground">
										{t("settings.connections.fields.endpoint")}:
									</strong>{" "}
									<span className="font-mono">
										{endpoint ?? t("settings.connections.notConnected")}
									</span>
								</p>
								<p>
									<strong className="text-foreground">
										{t("settings.connections.fields.remotePort")}:
									</strong>{" "}
									<span className="font-mono">{environment.remotePort}</span>
								</p>
								<p>
									<strong className="text-foreground">
										{t("settings.connections.fields.localPort")}:
									</strong>{" "}
									<span className="font-mono">
										{environment.localPort ?? t("settings.connections.autoPort")}
									</span>
								</p>
								<p>
									<strong className="text-foreground">
										{t("settings.connections.fields.health")}:
									</strong>{" "}
									<span className="font-mono">
										{probe?.healthStatus === "ok"
											? t("settings.connections.health.ok")
											: probe?.healthStatus === "degraded"
												? t("settings.connections.health.degraded")
												: t("settings.connections.health.unknown")}
									</span>
								</p>
								<p>
									<strong className="text-foreground">
										{t("settings.connections.fields.backend")}:
									</strong>{" "}
									<span className="font-mono">
										{probe?.statusSummary ??
											probe?.daemonStatus ??
											t("settings.connections.notConnected")}
									</span>
								</p>
								<p>
									<strong className="text-foreground">
										{t("settings.connections.fields.terminalPersistence")}:
									</strong>{" "}
									<span className="font-mono">{terminalPersistenceLabel}</span>
								</p>
							</div>

							{probe?.errorMessage ? (
								<p className="mt-3 text-[11px] text-destructive">{probe.errorMessage}</p>
							) : null}
						</div>
					);
				})}
			</div>
		</section>
	);
}
