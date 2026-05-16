import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
	CheckCircle2,
	CircleHelp,
	Copy,
	HeartPulse,
	KeyRound,
	Loader2,
	Play,
	QrCode,
	RefreshCw,
	Server,
	Square,
	TerminalSquare,
	Trash2,
	Wifi,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import {
	daemonGetStatus,
	daemonHealth,
	getCurrentBackendTarget,
} from "@/lib/daemon-api";
import {
	bootstrapRemoteSshBinary,
	closeRemoteMobileAccess,
	launchRemoteSshTunnel,
	listRemoteSshTunnels,
	openRemoteMobileAccess,
	preflightRemoteSsh,
	stopRemoteSshTunnel,
	type RemoteMobileAccessSnapshot,
	type RemotePreflightSnapshot,
	type RemoteTunnelSnapshot,
} from "@/lib/remote-api";
import { RemoteAccessQr } from "./remote-access-qr";
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

type MobileAccessSession = {
	environmentId: string;
	label: string;
	info: RemoteMobileAccessSnapshot;
};

function trimTrailingSlash(value: string) {
	return value.endsWith("/") ? value.slice(0, -1) : value;
}

function buildRemoteHealthUrl(endpoint: string) {
	return `${trimTrailingSlash(endpoint)}/health`;
}

function buildRemoteRpcUrl(endpoint: string) {
	return `${trimTrailingSlash(endpoint)}/rpc`;
}

function buildMobileAccessPayload(endpoint: string, bearerToken: string) {
	return [
		"DCC remote access",
		`endpoint=${endpoint}`,
		`bearerToken=${bearerToken}`,
		`health=${buildRemoteHealthUrl(endpoint)}`,
		`rpc=${buildRemoteRpcUrl(endpoint)}`,
	].join("\n");
}

function buildMobileAccessCurl(endpoint: string, bearerToken: string) {
	return `curl -H "Authorization: Bearer ${bearerToken}" -H "Content-Type: application/json" -d '{"method":"daemon.health","params":{}}' ${buildRemoteRpcUrl(endpoint)}`;
}

function tmuxInstallCommand(platformName: string | null) {
	switch ((platformName ?? "").trim()) {
		case "Darwin":
			return "brew install tmux";
		case "Linux":
			return "sudo apt-get install -y tmux";
		default:
			return "install tmux with your system package manager";
	}
}

function remotePreflightRecommendations(
	t: (key: string, options?: Record<string, unknown>) => string,
	preflight: RemotePreflightSnapshot | null,
	remoteCommand: string,
) {
	if (!preflight) {
		return [];
	}

	const items: string[] = [];
	if (!preflight.sshReachable) {
		items.push(t("settings.connections.recommendationItems.sshUnavailable"));
		return items;
	}
	if (!preflight.remoteCommandFound) {
		items.push(
			t("settings.connections.recommendationItems.remoteCommandMissing", {
				remoteCommand,
			}),
		);
	}
	if (preflight.binaryCompatible === false) {
		items.push(
			t("settings.connections.recommendationItems.binaryIncompatible", {
				platform: preflight.platformName ?? "unknown",
				arch: preflight.platformArch ?? "unknown",
			}),
		);
	}
	if (preflight.tmuxAvailable === false) {
		items.push(
			t("settings.connections.recommendationItems.tmuxMissing", {
				command: tmuxInstallCommand(preflight.platformName),
			}),
		);
	}
	if (items.length === 0) {
		items.push(t("settings.connections.recommendationItems.ready"));
	}
	return items;
}

function remotePreflightFixCommand(
	preflight: RemotePreflightSnapshot | null,
	remoteCommand: string,
) {
	if (!preflight || !preflight.sshReachable) {
		return null;
	}
	if (preflight.binaryCompatible === false) {
		return null;
	}
	if (!preflight.remoteCommandFound) {
		return `command -v ${remoteCommand} || echo "${remoteCommand} is missing from PATH"`;
	}
	if (preflight.tmuxAvailable === false) {
		return tmuxInstallCommand(preflight.platformName);
	}
	return null;
}

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

function getRemoteEnvironmentErrorMessage(error: unknown): string {
	if (typeof error === "string" && error.trim().length > 0) {
		return error.trim();
	}
	if (error && typeof error === "object") {
		const candidate = error as {
			message?: unknown;
			error?: unknown;
			data?: { message?: unknown; error?: unknown } | null;
			toString?: () => string;
		};
		if (typeof candidate.message === "string" && candidate.message.trim().length > 0) {
			return candidate.message.trim();
		}
		if (typeof candidate.error === "string" && candidate.error.trim().length > 0) {
			return candidate.error.trim();
		}
		if (
			candidate.data &&
			typeof candidate.data === "object" &&
			typeof candidate.data.message === "string" &&
			candidate.data.message.trim().length > 0
		) {
			return candidate.data.message.trim();
		}
		if (
			candidate.data &&
			typeof candidate.data === "object" &&
			typeof candidate.data.error === "string" &&
			candidate.data.error.trim().length > 0
		) {
			return candidate.data.error.trim();
		}
		if (typeof candidate.toString === "function") {
			const text = candidate.toString();
			if (typeof text === "string" && text !== "[object Object]" && text.trim().length > 0) {
				return text.trim();
			}
		}
	}
	return "Operation failed";
}

type InfoRowProps = {
	icon?: ReactNode;
	label: string;
	value: string;
	monoFamily?: boolean;
	onCopy?: () => void;
	copyLabel?: string;
};

function InfoRow({
	icon,
	label,
	value,
	monoFamily,
	onCopy,
	copyLabel,
}: InfoRowProps) {
	return (
		<div className="rounded-lg border border-border/60 bg-background">
			<div className="flex items-center justify-between gap-2 px-3 py-1.5">
				<div className="flex min-w-0 items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
					{icon}
					<span className="truncate">{label}</span>
				</div>
				{onCopy ? (
					<Button
						type="button"
						variant="ghost"
						size="xs"
						className="shrink-0 text-[11px]"
						onClick={onCopy}
					>
						<Copy className="size-3" />
						{copyLabel}
					</Button>
				) : null}
			</div>
			<div className="border-t border-border/50 px-3 py-2">
				<p
					className={
						monoFamily
							? "break-all font-mono text-[11px] leading-relaxed text-foreground/90"
							: "break-all text-[12px] leading-relaxed text-foreground/90"
					}
				>
					{value}
				</p>
			</div>
		</div>
	);
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
	const [preflightBusyId, setPreflightBusyId] = useState<string | null>(null);
	const [bootstrapBusyId, setBootstrapBusyId] = useState<string | null>(null);
	const [preflights, setPreflights] = useState<Record<string, RemotePreflightSnapshot>>({});
	const [isRefreshing, setIsRefreshing] = useState(false);
	const [localBackendProbe, setLocalBackendProbe] = useState<RemoteProbe | null>(null);
	const [docsOpen, setDocsOpen] = useState(false);
	const [mobileAccessBusyId, setMobileAccessBusyId] = useState<string | null>(null);
	const [mobileAccessSession, setMobileAccessSession] = useState<MobileAccessSession | null>(
		null,
	);

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
						errorMessage: getRemoteEnvironmentErrorMessage(error),
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
				toast.error(getRemoteEnvironmentErrorMessage(error));
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
							errorMessage: getRemoteEnvironmentErrorMessage(error),
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
			remoteVersion: null,
			remoteProtocolVersion: null,
			protocolCompatible: null,
		};
		const updated = [next, ...environments];
		persistEnvironments(updated);
		if (activeEnvironmentId && !updated.some((environment) => environment.id === activeEnvironmentId)) {
			setActiveEnvironment(null);
		}
		setDraft(defaultDraft());
	};

	const handleDelete = async (environment: SavedRemoteEnvironment) => {
		if (mobileAccessSession?.environmentId === environment.id) {
			await closeMobileAccessSession();
		}
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
				remoteCommand: tunnel.remoteCommand,
				localPort: tunnel.localPort,
				remotePort: tunnel.remotePort,
				bearerToken: tunnel.bearerToken,
				endpoint: tunnel.endpoint,
				lastStartedAt: tunnel.startedAt,
				tmuxAvailable: tunnel.tmuxAvailable,
				remoteVersion: tunnel.remoteVersion,
				remoteProtocolVersion: tunnel.remoteProtocolVersion,
				protocolCompatible: tunnel.protocolCompatible,
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
				toast.error(getRemoteEnvironmentErrorMessage(error));
			} finally {
				setBusyId(null);
			}
		};

	const handleDisconnect = async (environment: SavedRemoteEnvironment) => {
		setBusyId(environment.id);
		try {
			if (mobileAccessSession?.environmentId === environment.id) {
				await closeMobileAccessSession();
			}
			await stopRemoteSshTunnel(environment.id);
			setTunnels((current) => {
				const copy = { ...current };
				delete copy[environment.id];
				return copy;
			});
			} catch (error) {
				toast.error(getRemoteEnvironmentErrorMessage(error));
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
				toast.error(getRemoteEnvironmentErrorMessage(error));
			}
		};

	const copyText = async (value: string, successMessage: string) => {
		try {
			await navigator.clipboard.writeText(value);
			toast.success(successMessage);
			} catch (error) {
				toast.error(getRemoteEnvironmentErrorMessage(error));
			}
		};

	const copyFixCommand = async (
		environment: SavedRemoteEnvironment,
		command: string,
	) => {
		await copyText(
			command,
			t("settings.connections.copyFixCommandSuccess", {
				label: environment.label,
			}),
		);
	};

	const closeMobileAccessSession = async () => {
		const current = mobileAccessSession;
		setMobileAccessSession(null);
		if (!current) {
			return;
		}
		try {
			await closeRemoteMobileAccess(current.environmentId);
			} catch (error) {
				toast.error(getRemoteEnvironmentErrorMessage(error));
			}
		};

	const handleOpenMobileAccess = async (environment: SavedRemoteEnvironment) => {
		if (mobileAccessSession && mobileAccessSession.environmentId !== environment.id) {
			await closeMobileAccessSession();
		}

		setMobileAccessBusyId(environment.id);
		try {
			const info = await openRemoteMobileAccess(environment.id);
			setMobileAccessSession({
				environmentId: environment.id,
				label: environment.label,
				info,
			});
			} catch (error) {
				toast.error(getRemoteEnvironmentErrorMessage(error));
			} finally {
				setMobileAccessBusyId(null);
			}
		};

	const handlePreflight = async (environment: SavedRemoteEnvironment) => {
		setPreflightBusyId(environment.id);
		try {
			const result = await preflightRemoteSsh({
				sshTarget: environment.sshTarget,
				remoteCommand: environment.remoteCommand,
			});
			setPreflights((current) => ({
				...current,
				[environment.id]: result,
			}));
			if (result.tmuxAvailable !== null) {
				const updatedEnvironment: SavedRemoteEnvironment = {
					...environment,
					tmuxAvailable: result.tmuxAvailable,
				};
				persistEnvironments(
					environments.map((candidate) =>
						candidate.id === environment.id ? updatedEnvironment : candidate,
					),
				);
			}
			if (
				result.sshReachable &&
				result.remoteCommandFound &&
				result.binaryCompatible !== false
			) {
				toast.success(
					t("settings.connections.preflightOk", {
						label: environment.label,
					}),
				);
			} else {
				toast.error(
					t("settings.connections.preflightError", {
						label: environment.label,
					}),
				);
			}
			} catch (error) {
				toast.error(getRemoteEnvironmentErrorMessage(error));
			} finally {
				setPreflightBusyId(null);
			}
		};

	const handleBootstrapRuntime = async (environment: SavedRemoteEnvironment) => {
		setBootstrapBusyId(environment.id);
		try {
			const result = await bootstrapRemoteSshBinary({
				sshTarget: environment.sshTarget,
			});
			const updatedEnvironment: SavedRemoteEnvironment = {
				...environment,
				remoteCommand: result.remoteCommand,
				tmuxAvailable: result.tmuxAvailable,
				remoteVersion: null,
				remoteProtocolVersion: null,
				protocolCompatible: null,
			};
			persistEnvironments(
				environments.map((candidate) =>
					candidate.id === environment.id ? updatedEnvironment : candidate,
				),
			);
			toast.success(
				t("settings.connections.bootstrapSuccess", {
					label: environment.label,
				}),
			);
			const refreshedPreflight = await preflightRemoteSsh({
				sshTarget: environment.sshTarget,
				remoteCommand: result.remoteCommand,
			});
			setPreflights((current) => ({
				...current,
				[environment.id]: refreshedPreflight,
			}));
			} catch (error) {
				toast.error(
					t("settings.connections.bootstrapError", {
						label: environment.label,
					}),
				);
				toast.error(getRemoteEnvironmentErrorMessage(error));
			} finally {
				setBootstrapBusyId(null);
			}
		};

	const mobileAccessEndpoint = mobileAccessSession?.info.lanEndpoint ?? null;
	const mobileAccessPayload =
		mobileAccessEndpoint && mobileAccessSession
			? buildMobileAccessPayload(
					mobileAccessEndpoint,
					mobileAccessSession.info.bearerToken,
				)
			: null;
	const mobileAccessCurl =
		mobileAccessEndpoint && mobileAccessSession
			? buildMobileAccessCurl(mobileAccessEndpoint, mobileAccessSession.info.bearerToken)
			: null;

	return (
		<section className="space-y-4">
			<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
				<div className="flex items-start justify-between gap-4">
					<div className="min-w-0">
						<div className="flex items-center gap-2">
							<h3 className="text-[14px] font-medium text-foreground">
								{t("settings.connections.title")}
							</h3>
							<TooltipProvider>
								<Tooltip>
									<TooltipTrigger asChild>
										<Button
											type="button"
											variant="ghost"
											size="icon-xs"
											onClick={() => setDocsOpen(true)}
											aria-label={t("settings.connections.helpTooltip")}
										>
											<CircleHelp className="size-3.5" />
										</Button>
									</TooltipTrigger>
									<TooltipContent>
										{t("settings.connections.helpTooltip")}
									</TooltipContent>
								</Tooltip>
							</TooltipProvider>
						</div>
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
					const isPreflightBusy = preflightBusyId === environment.id;
					const isBootstrapBusy = bootstrapBusyId === environment.id;
					const isRunning = tunnel?.status === "running";
					const isActive = environment.id === activeEnvironmentId;
					const endpoint = tunnel?.endpoint ?? environment.endpoint;
					const preflight = preflights[environment.id] ?? null;
					const tmuxAvailable = tunnel?.tmuxAvailable ?? environment.tmuxAvailable;
					const remoteVersion = tunnel?.remoteVersion ?? environment.remoteVersion;
					const remoteProtocolVersion =
						tunnel?.remoteProtocolVersion ?? environment.remoteProtocolVersion;
					const protocolCompatible =
						tunnel?.protocolCompatible ?? environment.protocolCompatible;
					const terminalPersistenceLabel =
						tmuxAvailable === true
							? t("settings.connections.persistence.tmux")
							: tmuxAvailable === false
								? t("settings.connections.persistence.pty")
								: t("settings.connections.persistence.unknown");
					const recommendations = remotePreflightRecommendations(
						t,
						preflight,
						environment.remoteCommand,
					);
					const fixCommand = remotePreflightFixCommand(
						preflight,
						environment.remoteCommand,
					);

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
										disabled={isPreflightBusy}
										onClick={() => void handlePreflight(environment)}
									>
										{isPreflightBusy ? (
											<Loader2 className="size-3.5 animate-spin" />
										) : (
											<RefreshCw className="size-3.5" />
										)}
										{t("settings.connections.check")}
									</Button>
									<Button
										type="button"
										variant="outline"
										size="sm"
										disabled={isBootstrapBusy}
										onClick={() => void handleBootstrapRuntime(environment)}
									>
										{isBootstrapBusy ? (
											<Loader2 className="size-3.5 animate-spin" />
										) : (
											<Play className="size-3.5" />
										)}
										{t("settings.connections.bootstrapRuntime")}
									</Button>
									<Button
										type="button"
										variant="outline"
										size="sm"
										disabled={mobileAccessBusyId === environment.id || !isRunning}
										onClick={() => void handleOpenMobileAccess(environment)}
									>
										{mobileAccessBusyId === environment.id ? (
											<Loader2 className="size-3.5 animate-spin" />
										) : (
											<QrCode className="size-3.5" />
										)}
										{t("settings.connections.mobileAccess")}
									</Button>
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
										{t("settings.connections.fields.runtime")}:
									</strong>{" "}
									<span className="font-mono">
										{remoteVersion || remoteProtocolVersion
											? [
													remoteVersion ? `v${remoteVersion}` : null,
													remoteProtocolVersion
														? `proto ${remoteProtocolVersion}`
														: null,
													protocolCompatible === false ? "mismatch" : null,
												]
													.filter(Boolean)
													.join(" · ")
											: t("settings.connections.notConnected")}
									</span>
								</p>
								<p>
									<strong className="text-foreground">
										{t("settings.connections.fields.terminalPersistence")}:
									</strong>{" "}
									<span className="font-mono">{terminalPersistenceLabel}</span>
								</p>
								<p>
									<strong className="text-foreground">
										{t("settings.connections.fields.preflight")}:
									</strong>{" "}
									<span className="font-mono">
										{preflight
											? [
													preflight.sshReachable ? "ssh" : "ssh-fail",
													preflight.remoteCommandFound ? "dccd-http" : "missing-cmd",
													preflight.tmuxAvailable === true
														? "tmux"
														: preflight.tmuxAvailable === false
															? "no-tmux"
															: "tmux-unknown",
													preflight.platformName && preflight.platformArch
														? `${preflight.platformName}/${preflight.platformArch}`
														: null,
													preflight.binaryCompatible === true
														? "bin-ok"
														: preflight.binaryCompatible === false
															? "bin-mismatch"
															: null,
												]
													.filter(Boolean)
													.join(" · ")
											: t("settings.connections.health.unknown")}
									</span>
								</p>
							</div>

							{probe?.errorMessage ? (
								<p className="mt-3 text-[11px] text-destructive">{probe.errorMessage}</p>
							) : null}
							{preflight?.errorMessage ? (
								<p className="mt-2 text-[11px] text-destructive">
									{preflight.errorMessage}
								</p>
							) : null}
							{preflight ? (
								<div className="mt-3 rounded-lg border border-border/50 bg-muted/20 p-3 text-[11px] text-muted-foreground">
									<div className="flex flex-wrap items-center justify-between gap-2">
										<p className="font-medium text-foreground">
											{t("settings.connections.recommendations")}
										</p>
										{fixCommand ? (
											<Button
												type="button"
												variant="outline"
												size="xs"
												onClick={() => void copyFixCommand(environment, fixCommand)}
											>
												<Copy className="size-3.5" />
												{t("settings.connections.copyFixCommand")}
											</Button>
										) : null}
									</div>
									<div className="mt-1 space-y-1">
										{recommendations.map((recommendation) => (
											<p key={recommendation}>{recommendation}</p>
										))}
									</div>
									{fixCommand ? (
										<p className="mt-2 font-mono text-[10px] text-foreground/80">
											{fixCommand}
										</p>
									) : null}
								</div>
							) : null}
						</div>
					);
				})}
			</div>

			<Dialog open={docsOpen} onOpenChange={setDocsOpen}>
				<DialogContent className="max-w-xl">
					<DialogHeader>
						<DialogTitle>{t("settings.connections.helpTitle")}</DialogTitle>
						<DialogDescription>
							{t("settings.connections.helpBody")}
						</DialogDescription>
					</DialogHeader>
					<div className="space-y-3 text-[12px] leading-relaxed text-muted-foreground">
						<div className="rounded-lg border border-border/60 bg-muted/20 p-3">
							<p className="font-medium text-foreground">
								{t("settings.connections.helpCreateTitle")}
							</p>
							<p className="mt-1">{t("settings.connections.helpCreateBody")}</p>
						</div>
						<div className="rounded-lg border border-border/60 bg-muted/20 p-3">
							<p className="font-medium text-foreground">
								{t("settings.connections.helpCheckTitle")}
							</p>
							<p className="mt-1">{t("settings.connections.helpCheckBody")}</p>
						</div>
						<div className="rounded-lg border border-border/60 bg-muted/20 p-3">
							<p className="font-medium text-foreground">
								{t("settings.connections.helpConnectTitle")}
							</p>
							<p className="mt-1">{t("settings.connections.helpConnectBody")}</p>
						</div>
						<div className="rounded-lg border border-border/60 bg-muted/20 p-3">
							<p className="font-medium text-foreground">
								{t("settings.connections.helpMobileTitle")}
							</p>
							<p className="mt-1">{t("settings.connections.helpMobileBody")}</p>
						</div>
						<p className="text-[11px]">{t("settings.connections.helpSecurity")}</p>
					</div>
					<DialogFooter showCloseButton />
				</DialogContent>
			</Dialog>

			<Dialog
				open={mobileAccessSession !== null}
				onOpenChange={(open) => {
					if (!open) {
						void closeMobileAccessSession();
					}
				}}
			>
				<DialogContent className="flex max-h-[90vh] w-[calc(100%-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl">
					<DialogHeader className="border-b border-border/50 px-6 py-4">
						<DialogTitle className="text-[15px]">
							{t("settings.connections.mobileAccessTitle", {
								label: mobileAccessSession?.label ?? "",
							})}
						</DialogTitle>
						<DialogDescription className="text-[12px]">
							{t("settings.connections.mobileAccessBody")}
						</DialogDescription>
					</DialogHeader>
					{mobileAccessSession ? (
						<div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
							<div className="grid gap-6 md:grid-cols-[240px_minmax(0,1fr)]">
								<div className="flex flex-col items-center gap-3">
									{mobileAccessPayload ? (
										<RemoteAccessQr value={mobileAccessPayload} size={220} />
									) : (
										<div className="flex h-[220px] w-[220px] items-center justify-center rounded-xl border border-dashed border-border/70 bg-muted/20 p-4 text-center text-[11px] leading-relaxed text-muted-foreground">
											{t("settings.connections.mobileAccessNoLan")}
										</div>
									)}
									<p className="text-center text-[11px] leading-relaxed text-muted-foreground">
										{t("settings.connections.mobileAccessQrHint")}
									</p>
									{mobileAccessPayload ? (
										<Button
											type="button"
											variant="outline"
											size="sm"
											className="w-full"
											onClick={() =>
												void copyText(
													mobileAccessPayload,
													t("settings.connections.copyPayloadSuccess"),
												)
											}
										>
											<Copy className="size-3.5" />
											{t("settings.connections.copyPayload")}
										</Button>
									) : null}
								</div>
								<div className="min-w-0 space-y-3">
									<div className="rounded-lg border border-border/60 bg-muted/15 px-3 py-2.5 text-[12px] leading-relaxed text-muted-foreground">
										<p className="font-medium text-foreground">
											{t("settings.connections.mobileAccessWhileOpen")}
										</p>
										<p className="mt-1">
											{t("settings.connections.mobileAccessWhileOpenBody")}
										</p>
									</div>

									<InfoRow
										icon={<Wifi className="size-3.5" />}
										label={t("settings.connections.mobileAccessLanEndpoint")}
										value={
											mobileAccessEndpoint ??
											t("settings.connections.mobileAccessNoLan")
										}
										onCopy={
											mobileAccessEndpoint
												? () =>
														void copyText(
															mobileAccessEndpoint,
															t("settings.connections.copyLanEndpointSuccess"),
														)
												: undefined
										}
										copyLabel={t("settings.connections.copyLanEndpoint")}
									/>

									{mobileAccessSession.info.lanEndpoints.length > 1 ? (
										<InfoRow
											label={t(
												"settings.connections.mobileAccessAlternateEndpoints",
											)}
											value={mobileAccessSession.info.lanEndpoints.join(" · ")}
										/>
									) : null}

									<InfoRow
										icon={<Server className="size-3.5" />}
										label={t("settings.connections.mobileAccessLocalEndpoint")}
										value={mobileAccessSession.info.localEndpoint}
									/>

									<InfoRow
										icon={<KeyRound className="size-3.5" />}
										label={t("settings.connections.mobileAccessToken")}
										value={mobileAccessSession.info.bearerToken}
										monoFamily
										onCopy={() =>
											void copyText(
												mobileAccessSession.info.bearerToken,
												t("settings.connections.copyTokenSuccess"),
											)
										}
										copyLabel={t("settings.connections.copyToken")}
									/>

									<InfoRow
										icon={<HeartPulse className="size-3.5" />}
										label={t("settings.connections.mobileAccessHealth")}
										value={
											mobileAccessEndpoint
												? buildRemoteHealthUrl(mobileAccessEndpoint)
												: t("settings.connections.mobileAccessNoLan")
										}
									/>

									{mobileAccessCurl ? (
										<div className="overflow-hidden rounded-lg border border-border/60 bg-background">
											<div className="flex items-center justify-between gap-2 px-3 py-1.5">
												<div className="flex min-w-0 items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
													<TerminalSquare className="size-3.5" />
													<span className="truncate">
														{t("settings.connections.mobileAccessCurl")}
													</span>
												</div>
												<Button
													type="button"
													variant="ghost"
													size="xs"
													className="shrink-0 text-[11px]"
													onClick={() =>
														void copyText(
															mobileAccessCurl,
															t("settings.connections.copyCurlSuccess"),
														)
													}
												>
													<Copy className="size-3" />
													{t("settings.connections.copyCurl")}
												</Button>
											</div>
											<pre className="m-0 max-h-32 overflow-auto border-t border-border/50 px-3 py-2 font-mono text-[10px] leading-relaxed whitespace-pre-wrap break-all text-foreground/80">
												{mobileAccessCurl}
											</pre>
										</div>
									) : null}
								</div>
							</div>
						</div>
					) : null}
					<DialogFooter className="border-t border-border/50 px-6 py-3">
						<Button
							type="button"
							variant="outline"
							onClick={() => void closeMobileAccessSession()}
						>
							{t("settings.connections.mobileAccessClose")}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</section>
	);
}
