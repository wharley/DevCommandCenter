import { useEffect, useMemo, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
	AlertTriangle,
	CheckCircle2,
	Copy,
	KeyRound,
	Loader2,
	QrCode,
	RefreshCw,
	Smartphone,
	Trash2,
	XCircle,
} from "lucide-react";
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
import { RemoteAccessQr } from "./remote-access-qr";
import {
	pairAuditLog,
	pairGetEndpoints,
	pairInit,
	pairListDevices,
	pairRevokeDevice,
	type AdvertisedEndpoint,
	type AuditEntry,
	type PairedDevice,
	type PairingChallenge,
} from "@/lib/pairing-api";

/**
 * Builds the pairing URL the QR encodes. The landing page is served by the
 * desktop backend itself (same origin as the API) so the phone never hits a
 * mixed-content / CORS wall. `backendUrl` should already be the LAN URL of
 * the desktop, e.g. http://192.168.1.42:9876.
 */
function buildPairUrl(backendUrl: string, nonce: string): string {
	const params = new URLSearchParams({ be: backendUrl, nonce });
	return `${backendUrl.replace(/\/$/, "")}/m/pair#${params.toString()}`;
}

function formatRelative(iso: string | null): string {
	if (!iso) return "—";
	try {
		const date = new Date(iso);
		const diff = (Date.now() - date.getTime()) / 1000;
		if (diff < 60) return "agora";
		if (diff < 3600) return `${Math.floor(diff / 60)} min`;
		if (diff < 86400) return `${Math.floor(diff / 3600)}h`;
		const days = Math.floor(diff / 86400);
		if (days < 30) return `${days}d`;
		return date.toLocaleDateString("pt-BR");
	} catch {
		return iso;
	}
}

export function PairedDevicesPanel({
	defaultBackendUrl,
}: {
	/**
	 * Loopback URL kept as a last-resort fallback (e.g. tests, dev shells).
	 * The real backend URL shown to mobile clients comes from `pair_get_lan_url`
	 * and is the LAN address of this machine.
	 */
	defaultBackendUrl: string;
}) {
	const [devices, setDevices] = useState<PairedDevice[]>([]);
	const [loading, setLoading] = useState(false);
	const [revokingId, setRevokingId] = useState<string | null>(null);
	const [pairDialogOpen, setPairDialogOpen] = useState(false);
	const [endpoints, setEndpoints] = useState<AdvertisedEndpoint[] | null>(null);

	const refreshEndpoints = () => {
		void pairGetEndpoints()
			.then((res) => setEndpoints(res.endpoints))
			.catch(() => setEndpoints([]));
	};

	const refresh = async () => {
		setLoading(true);
		try {
			const response = await pairListDevices(false);
			setDevices(response.devices);
		} catch (err) {
			toast.error(
				err instanceof Error ? err.message : "Falha ao carregar dispositivos",
			);
		} finally {
			setLoading(false);
		}
	};

	useEffect(() => {
		void refresh();
		refreshEndpoints();
	}, []);

	useEffect(() => {
		let unlisten: UnlistenFn | undefined;
		void (async () => {
			unlisten = await listen<AuditEntry>("pair-audit-event", (event) => {
				const payload = event.payload;
				switch (payload.event) {
					case "pair":
						toast.success("Novo dispositivo pareado", {
							description: payload.userAgent ?? payload.deviceId ?? undefined,
						});
						void refresh();
						break;
					case "pin_locked":
						toast.warning("Tentativa de brute-force detectada", {
							description: payload.ip
								? `Nonce travado para IP ${payload.ip}`
								: "Nonce travado após 5 PINs incorretos",
						});
						break;
					case "revoke":
						void refresh();
						break;
				}
			});
		})();
		return () => {
			unlisten?.();
		};
	}, []);

	const handleRevoke = async (device: PairedDevice) => {
		setRevokingId(device.deviceId);
		try {
			await pairRevokeDevice(device.deviceId);
			toast.success(`${device.deviceName} desvinculado`);
			await refresh();
		} catch (err) {
			toast.error(err instanceof Error ? err.message : "Falha ao revogar");
		} finally {
			setRevokingId(null);
		}
	};

	return (
		<section className="space-y-3">
			<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
				<div className="flex items-start justify-between gap-3">
					<div className="min-w-0">
						<div className="flex items-center gap-2">
							<Smartphone className="size-4 text-muted-foreground" strokeWidth={1.9} />
							<h3 className="text-[14px] font-medium text-foreground">
								Dispositivos pareados
							</h3>
						</div>
						<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
							Celulares e tablets autenticam por chave pública (ECDSA P-256). A chave
							privada nunca sai do dispositivo. Você pode revogar individualmente a
							qualquer momento.
						</p>
					</div>
					<Badge variant={devices.length > 0 ? "success" : "outline"} className="h-7 shrink-0 px-2.5 text-[11px] font-normal">
						{devices.length}
					</Badge>
				</div>

				<div className="mt-3 flex flex-wrap gap-2">
					<Button
						type="button"
						size="sm"
						onClick={() => setPairDialogOpen(true)}
					>
						<QrCode className="size-3.5" />
						Parear novo dispositivo
					</Button>
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={() => void refresh()}
					>
						<RefreshCw className={`size-3.5 ${loading ? "animate-spin" : ""}`} />
						Atualizar
					</Button>
				</div>
			</div>

			{devices.length === 0 && !loading ? (
				<div className="rounded-xl border border-dashed border-border/70 p-4 text-center text-[12px] text-muted-foreground">
					Nenhum dispositivo pareado ainda.
				</div>
			) : (
				<div className="overflow-hidden rounded-xl border border-border/60">
					<table className="w-full text-[12px]">
						<thead className="bg-muted/30 text-[11px] text-muted-foreground">
							<tr>
								<th className="px-3 py-2 text-left font-medium">Dispositivo</th>
								<th className="px-3 py-2 text-left font-medium">Último uso</th>
								<th className="px-3 py-2 text-left font-medium">Pareado em</th>
								<th className="px-3 py-2 text-right font-medium" />
							</tr>
						</thead>
						<tbody>
							{devices.map((device) => (
								<tr
									key={device.deviceId}
									className="border-t border-border/40 transition-colors hover:bg-muted/10"
								>
									<td className="px-3 py-2.5">
										<div className="flex items-center gap-2">
											<KeyRound className="size-3.5 shrink-0 text-muted-foreground" />
											<div className="min-w-0">
												<p className="truncate font-medium text-foreground">
													{device.deviceName}
												</p>
												{device.userAgent ? (
													<p className="truncate text-[10px] text-muted-foreground">
														{device.userAgent}
													</p>
												) : null}
											</div>
										</div>
									</td>
									<td className="px-3 py-2.5 text-muted-foreground">
										{formatRelative(device.lastUsedAt)}
									</td>
									<td className="px-3 py-2.5 text-muted-foreground">
										{formatRelative(device.createdAt)}
									</td>
									<td className="px-3 py-2.5 text-right">
										<Button
											type="button"
											variant="ghost"
											size="xs"
											disabled={revokingId === device.deviceId}
											onClick={() => void handleRevoke(device)}
											className="text-destructive hover:text-destructive"
										>
											{revokingId === device.deviceId ? (
												<Loader2 className="size-3.5 animate-spin" />
											) : (
												<Trash2 className="size-3.5" />
											)}
											Revogar
										</Button>
									</td>
								</tr>
							))}
						</tbody>
					</table>
				</div>
			)}

			<PairDeviceDialog
				open={pairDialogOpen}
				onClose={() => {
					setPairDialogOpen(false);
					void refresh();
				}}
				endpoints={endpoints}
				fallbackUrl={defaultBackendUrl}
				onRefreshEndpoints={refreshEndpoints}
			/>

			<AuditLogSection />
		</section>
	);
}

function AuditLogSection() {
	const [entries, setEntries] = useState<AuditEntry[]>([]);
	const [loading, setLoading] = useState(false);
	const [expanded, setExpanded] = useState(false);

	const refresh = async () => {
		setLoading(true);
		try {
			const res = await pairAuditLog(50);
			setEntries(res.entries);
		} catch (err) {
			toast.error(err instanceof Error ? err.message : "Falha ao carregar log");
		} finally {
			setLoading(false);
		}
	};

	useEffect(() => {
		if (expanded) {
			void refresh();
		}
	}, [expanded]);

	if (!expanded) {
		return (
			<button
				type="button"
				onClick={() => setExpanded(true)}
				className="w-full rounded-xl border border-border/60 bg-muted/10 px-4 py-2.5 text-left text-[12px] text-muted-foreground transition-colors hover:bg-muted/20"
			>
				Ver log de auditoria de pareamentos →
			</button>
		);
	}

	return (
		<div className="rounded-xl border border-border/60">
			<div className="flex items-center justify-between border-b border-border/40 px-4 py-2.5">
				<p className="text-[12px] font-medium text-foreground">
					Log de auditoria
				</p>
				<div className="flex items-center gap-1.5">
					<Button
						type="button"
						variant="ghost"
						size="xs"
						onClick={() => void refresh()}
					>
						<RefreshCw className={`size-3.5 ${loading ? "animate-spin" : ""}`} />
					</Button>
					<Button type="button" variant="ghost" size="xs" onClick={() => setExpanded(false)}>
						Fechar
					</Button>
				</div>
			</div>
			{entries.length === 0 ? (
				<p className="px-4 py-6 text-center text-[12px] text-muted-foreground">
					{loading ? "Carregando…" : "Nenhum evento registrado."}
				</p>
			) : (
				<ul className="max-h-72 divide-y divide-border/40 overflow-y-auto text-[11px]">
					{entries.map((entry) => (
						<AuditEntryRow key={entry.id} entry={entry} />
					))}
				</ul>
			)}
		</div>
	);
}

function AuditEntryRow({ entry }: { entry: AuditEntry }) {
	const meta = describeEvent(entry.event);
	return (
		<li className="flex items-start gap-3 px-4 py-2">
			<meta.icon className={`mt-0.5 size-3.5 shrink-0 ${meta.tone}`} aria-hidden />
			<div className="min-w-0 flex-1">
				<div className="flex items-center gap-2">
					<p className="font-medium text-foreground">{meta.label}</p>
					<span className="text-muted-foreground/70">{formatTime(entry.createdAt)}</span>
				</div>
				<div className="mt-0.5 flex flex-wrap gap-x-3 gap-y-0.5 text-[10px] text-muted-foreground">
					{entry.deviceId ? (
						<span className="font-mono">device {entry.deviceId.slice(0, 8)}…</span>
					) : null}
					{entry.ip ? <span className="font-mono">ip {entry.ip}</span> : null}
					{entry.userAgent ? (
						<span className="truncate">{entry.userAgent}</span>
					) : null}
				</div>
			</div>
		</li>
	);
}

function describeEvent(event: string): {
	label: string;
	icon: typeof CheckCircle2;
	tone: string;
} {
	switch (event) {
		case "pair":
			return { label: "Pareamento concluído", icon: CheckCircle2, tone: "text-emerald-500" };
		case "revoke":
			return { label: "Dispositivo revogado", icon: XCircle, tone: "text-amber-500" };
		case "pin_failure":
			return { label: "PIN incorreto", icon: AlertTriangle, tone: "text-amber-500" };
		case "pin_locked":
			return { label: "Nonce travado por brute-force", icon: AlertTriangle, tone: "text-red-500" };
		default:
			return { label: event, icon: CheckCircle2, tone: "text-muted-foreground" };
	}
}

function formatTime(iso: string): string {
	try {
		const d = new Date(iso);
		return d.toLocaleString("pt-BR", { dateStyle: "short", timeStyle: "medium" });
	} catch {
		return iso;
	}
}

function EndpointOption({
	endpoint,
	selected,
	onSelect,
}: {
	endpoint: AdvertisedEndpoint;
	selected: boolean;
	onSelect: () => void;
}) {
	const badge =
		endpoint.provider === "tailscale"
			? { text: "Tailscale", tone: "text-violet-400" }
			: endpoint.reachability === "lan"
				? { text: "LAN", tone: "text-emerald-400" }
				: { text: endpoint.label, tone: "text-muted-foreground" };
	return (
		<button
			type="button"
			onClick={onSelect}
			className={`flex w-full flex-col items-start gap-0.5 rounded-lg border px-3 py-2 text-left transition-colors ${
				selected
					? "border-foreground/40 bg-muted/30"
					: "border-border/60 hover:bg-muted/15"
			}`}
		>
			<div className="flex w-full items-center justify-between gap-2">
				<span className="text-[12px] font-medium text-foreground">{endpoint.label}</span>
				<span className={`text-[10px] font-medium uppercase tracking-wider ${badge.tone}`}>
					{badge.text}
				</span>
			</div>
			<span className="text-[11px] leading-snug text-muted-foreground">
				{endpoint.description}
			</span>
		</button>
	);
}

function PairDeviceDialog({
	open,
	onClose,
	endpoints,
	fallbackUrl,
	onRefreshEndpoints,
}: {
	open: boolean;
	onClose: () => void;
	/** Null = still loading; empty array = no usable endpoint detected. */
	endpoints: AdvertisedEndpoint[] | null;
	/** Last-resort URL (defaults to loopback) when discovery returns nothing. */
	fallbackUrl: string;
	onRefreshEndpoints: () => void;
}) {
	const [challenge, setChallenge] = useState<PairingChallenge | null>(null);
	const [generating, setGenerating] = useState(false);
	const [remainingSecs, setRemainingSecs] = useState(0);
	const [selectedEndpointId, setSelectedEndpointId] = useState<string | null>(null);

	// Endpoints reachable from a phone, in display order. Loopback is hidden
	// from the picker but still selectable as a debug fallback.
	const externalEndpoints = useMemo(
		() =>
			(endpoints ?? []).filter(
				(e) => e.reachability !== "loopback" && e.status !== "unavailable",
			),
		[endpoints],
	);

	// Pick a sensible default the first time endpoints land: prefer Tailscale
	// (works from any network) over LAN, but never overwrite a user choice.
	useEffect(() => {
		if (selectedEndpointId) return;
		if (!endpoints || endpoints.length === 0) return;
		const tailscale = endpoints.find((e) => e.provider === "tailscale" && e.status !== "unavailable");
		const lan = endpoints.find((e) => e.reachability === "lan");
		const any = externalEndpoints[0] ?? endpoints[0];
		setSelectedEndpointId((tailscale ?? lan ?? any)?.id ?? null);
	}, [endpoints, selectedEndpointId, externalEndpoints]);

	const selectedEndpoint =
		endpoints?.find((e) => e.id === selectedEndpointId) ?? null;
	const backendUrl = selectedEndpoint?.url ?? fallbackUrl;
	const noEndpointsDetected = endpoints !== null && externalEndpoints.length === 0;

	const generate = async () => {
		setGenerating(true);
		try {
			const result = await pairInit();
			setChallenge(result);
		} catch (err) {
			toast.error(
				err instanceof Error ? err.message : "Falha ao gerar pareamento",
			);
		} finally {
			setGenerating(false);
		}
	};

	useEffect(() => {
		if (open && !challenge && !noEndpointsDetected) {
			void generate();
		}
		if (!open) {
			setChallenge(null);
			setRemainingSecs(0);
			setSelectedEndpointId(null);
		}
	}, [open, noEndpointsDetected]);

	useEffect(() => {
		if (!challenge) return;
		const expires = new Date(challenge.expiresAt).getTime();
		const tick = () => {
			const secs = Math.max(0, Math.floor((expires - Date.now()) / 1000));
			setRemainingSecs(secs);
		};
		tick();
		const id = window.setInterval(tick, 1000);
		return () => window.clearInterval(id);
	}, [challenge]);

	const pairUrl = useMemo(() => {
		if (!challenge) return null;
		return buildPairUrl(backendUrl, challenge.nonce);
	}, [challenge, backendUrl]);

	const copyPairUrl = async () => {
		if (!pairUrl) return;
		try {
			await navigator.clipboard.writeText(pairUrl);
			toast.success("Link copiado");
		} catch {
			toast.error("Falha ao copiar");
		}
	};

	const expired = challenge !== null && remainingSecs <= 0;

	return (
		<Dialog open={open} onOpenChange={(o) => !o && onClose()}>
			<DialogContent className="flex max-h-[90vh] w-[calc(100%-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-md">
				<DialogHeader className="border-b border-border/50 px-6 py-4">
					<DialogTitle className="text-[15px]">Parear novo dispositivo</DialogTitle>
					<DialogDescription className="text-[12px]">
						Escaneie o QR code no celular e digite o PIN abaixo. O pareamento expira em 60 segundos.
					</DialogDescription>
				</DialogHeader>

				<div className="flex-1 overflow-y-auto px-6 py-5">
					{noEndpointsDetected ? (
						<div className="flex h-[280px] flex-col items-center justify-center gap-3 px-4 text-center">
							<AlertTriangle className="size-6 text-amber-500" strokeWidth={1.8} />
							<p className="text-[13px] font-medium text-foreground">
								Nenhuma rede alcançável detectada
							</p>
							<p className="text-[12px] leading-relaxed text-muted-foreground">
								O celular precisa de uma rota até este desktop. Conecte ao
								Wi-Fi (mesma rede do celular), ou instale o Tailscale em ambos
								para parear de qualquer lugar.
							</p>
							<Button
								type="button"
								size="sm"
								variant="outline"
								onClick={onRefreshEndpoints}
							>
								<RefreshCw className="size-3.5" />
								Verificar novamente
							</Button>
						</div>
					) : generating || !challenge ? (
						<div className="flex h-[280px] items-center justify-center">
							<Loader2 className="size-5 animate-spin text-muted-foreground" />
						</div>
					) : expired ? (
						<div className="flex h-[280px] flex-col items-center justify-center gap-3 text-center">
							<p className="text-[13px] font-medium text-foreground">
								PIN expirado
							</p>
							<p className="text-[12px] text-muted-foreground">
								Gere um novo para tentar novamente.
							</p>
							<Button
								type="button"
								size="sm"
								onClick={() => void generate()}
								disabled={generating}
							>
								<RefreshCw className="size-3.5" />
								Gerar novo
							</Button>
						</div>
					) : (
						<div className="flex flex-col items-center gap-4">
							{pairUrl ? (
								<RemoteAccessQr value={pairUrl} size={224} />
							) : null}

							<div className="w-full rounded-xl border border-border/60 bg-muted/15 p-4 text-center">
								<p className="text-[11px] uppercase tracking-wider text-muted-foreground">
									PIN
								</p>
								<p className="mt-1 font-mono text-[34px] font-semibold tracking-[0.35em] text-foreground">
									{challenge.pin}
								</p>
								<p className="mt-1 text-[11px] text-muted-foreground">
									expira em{" "}
									<span className="font-mono text-foreground/80">
										{remainingSecs}s
									</span>
								</p>
							</div>

							<Button
								type="button"
								variant="outline"
								size="sm"
								className="w-full"
								onClick={() => void copyPairUrl()}
							>
								<Copy className="size-3.5" />
								Copiar link do pareamento
							</Button>

							{externalEndpoints.length > 1 ? (
								<div className="w-full space-y-1.5">
									<p className="text-[11px] uppercase tracking-wider text-muted-foreground">
										Como o celular vai conectar
									</p>
									<div className="space-y-1.5">
										{externalEndpoints.map((ep) => (
											<EndpointOption
												key={ep.id}
												endpoint={ep}
												selected={ep.id === selectedEndpointId}
												onSelect={() => setSelectedEndpointId(ep.id)}
											/>
										))}
									</div>
								</div>
							) : selectedEndpoint ? (
								<p className="text-center text-[11px] leading-relaxed text-muted-foreground">
									{selectedEndpoint.description}
								</p>
							) : null}
							<p className="text-center font-mono text-[10px] text-muted-foreground/70">
								{backendUrl}
							</p>
						</div>
					)}
				</div>

				<DialogFooter className="border-t border-border/50 px-6 py-3">
					<Button type="button" variant="outline" onClick={onClose}>
						Fechar
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
