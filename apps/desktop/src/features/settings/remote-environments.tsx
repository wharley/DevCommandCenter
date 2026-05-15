import { useEffect, useMemo, useState } from "react";
import { Copy, Loader2, Play, RefreshCw, Square, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
	launchRemoteSshTunnel,
	listRemoteSshTunnels,
	stopRemoteSshTunnel,
	type RemoteTunnelSnapshot,
} from "@/lib/remote-api";

type SavedRemoteEnvironment = {
	id: string;
	label: string;
	sshTarget: string;
	remoteCommand: string;
	localPort: number | null;
	remotePort: number;
	bearerToken: string | null;
	endpoint: string | null;
	lastStartedAt: string | null;
};

type DraftEnvironment = {
	label: string;
	sshTarget: string;
	remoteCommand: string;
	localPort: string;
	remotePort: string;
};

const REMOTE_ENV_STORAGE_KEY = "dcc.remote.environments.v1";

function defaultDraft(): DraftEnvironment {
	return {
		label: "",
		sshTarget: "",
		remoteCommand: "dccd-http",
		localPort: "",
		remotePort: "9876",
	};
}

function readRemoteEnvironments(): SavedRemoteEnvironment[] {
	if (typeof window === "undefined") {
		return [];
	}

	try {
		const raw = window.localStorage.getItem(REMOTE_ENV_STORAGE_KEY);
		if (!raw) {
			return [];
		}
		const parsed = JSON.parse(raw);
		if (!Array.isArray(parsed)) {
			return [];
		}
		return parsed
			.map((value) => normalizeRemoteEnvironment(value))
			.filter((value): value is SavedRemoteEnvironment => value !== null);
	} catch {
		return [];
	}
}

function writeRemoteEnvironments(next: SavedRemoteEnvironment[]) {
	if (typeof window === "undefined") {
		return;
	}
	window.localStorage.setItem(REMOTE_ENV_STORAGE_KEY, JSON.stringify(next));
}

function normalizeRemoteEnvironment(value: unknown): SavedRemoteEnvironment | null {
	if (!value || typeof value !== "object") {
		return null;
	}

	const record = value as Record<string, unknown>;
	const id = typeof record.id === "string" ? record.id : "";
	const label = typeof record.label === "string" ? record.label : "";
	const sshTarget = typeof record.sshTarget === "string" ? record.sshTarget : "";
	if (!id || !label || !sshTarget) {
		return null;
	}

	return {
		id,
		label,
		sshTarget,
		remoteCommand:
			typeof record.remoteCommand === "string" && record.remoteCommand.trim()
				? record.remoteCommand
				: "dccd-http",
		localPort:
			typeof record.localPort === "number" && Number.isFinite(record.localPort)
				? record.localPort
				: null,
		remotePort:
			typeof record.remotePort === "number" && Number.isFinite(record.remotePort)
				? record.remotePort
				: 9876,
		bearerToken: typeof record.bearerToken === "string" ? record.bearerToken : null,
		endpoint: typeof record.endpoint === "string" ? record.endpoint : null,
		lastStartedAt: typeof record.lastStartedAt === "string" ? record.lastStartedAt : null,
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
	const [busyId, setBusyId] = useState<string | null>(null);
	const [isRefreshing, setIsRefreshing] = useState(false);

	const persistEnvironments = (next: SavedRemoteEnvironment[]) => {
		setEnvironments(next);
		writeRemoteEnvironments(next);
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
		};
		persistEnvironments([next, ...environments]);
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
		setTunnels((current) => {
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
			persistEnvironments(
				environments.map((candidate) =>
					candidate.id === environment.id
						? {
								...candidate,
								localPort: tunnel.localPort,
								remotePort: tunnel.remotePort,
								bearerToken: tunnel.bearerToken,
								endpoint: tunnel.endpoint,
								lastStartedAt: tunnel.startedAt,
							}
						: candidate,
				),
			);
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
					<Badge variant={activeCount > 0 ? "success" : "outline"} className="h-8 px-3 text-[12px] font-normal">
						{t("settings.connections.activeBadge", { count: activeCount })}
					</Badge>
				</div>
				<p className="mt-3 text-[11px] leading-relaxed text-muted-foreground">
					{t("settings.connections.requirements")}
				</p>
			</div>

			<div className="rounded-xl border border-border/60 p-4">
				<div className="grid gap-3 md:grid-cols-2">
					<div className="grid gap-1.5">
						<label className="text-[12px] font-medium text-foreground">
							{t("settings.connections.fields.label")}
						</label>
						<Input
							value={draft.label}
							onChange={(event) => setDraft((current) => ({ ...current, label: event.target.value }))}
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
								setDraft((current) => ({ ...current, remoteCommand: event.target.value }))
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
					const isBusy = busyId === environment.id;
					const isRunning = tunnel?.status === "running";
					const endpoint = tunnel?.endpoint ?? environment.endpoint;
					return (
						<div key={environment.id} className="rounded-xl border border-border/60 p-4">
							<div className="flex flex-wrap items-start justify-between gap-3">
								<div className="min-w-0">
									<div className="flex flex-wrap items-center gap-2">
										<h3 className="text-[14px] font-medium text-foreground">
											{environment.label}
										</h3>
										<Badge variant={isRunning ? "success" : "outline"}>
											{isRunning
												? t("settings.connections.status.running")
												: t("settings.connections.status.idle")}
										</Badge>
									</div>
									<p className="mt-1 break-all font-mono text-[11px] text-muted-foreground">
										{environment.sshTarget}
									</p>
								</div>

								<div className="flex flex-wrap gap-2">
									{isRunning ? (
										<Button
											type="button"
											variant="outline"
											size="sm"
											disabled={isBusy}
											onClick={() => void handleDisconnect(environment)}
										>
											{isBusy ? <Loader2 className="size-3.5 animate-spin" /> : <Square className="size-3.5" />}
											{t("settings.connections.disconnect")}
										</Button>
									) : (
										<Button
											type="button"
											size="sm"
											disabled={isBusy}
											onClick={() => void handleConnect(environment)}
										>
											{isBusy ? <Loader2 className="size-3.5 animate-spin" /> : <Play className="size-3.5" />}
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
									<strong className="text-foreground">{t("settings.connections.fields.remoteCommand")}:</strong>{" "}
									<span className="font-mono">{environment.remoteCommand}</span>
								</p>
								<p>
									<strong className="text-foreground">{t("settings.connections.fields.endpoint")}:</strong>{" "}
									<span className="font-mono">{endpoint ?? t("settings.connections.notConnected")}</span>
								</p>
								<p>
									<strong className="text-foreground">{t("settings.connections.fields.remotePort")}:</strong>{" "}
									<span className="font-mono">{environment.remotePort}</span>
								</p>
								<p>
									<strong className="text-foreground">{t("settings.connections.fields.localPort")}:</strong>{" "}
									<span className="font-mono">
										{environment.localPort ?? t("settings.connections.autoPort")}
									</span>
								</p>
							</div>
						</div>
					);
				})}
			</div>
		</section>
	);
}
