import { useEffect, useState } from "react";
import {
	AlertTriangle,
	CheckCircle2,
	KeyRound,
	Loader2,
	RefreshCw,
	Smartphone,
	Trash2,
	XCircle,
} from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { RelayAccessCard } from "./relay-pairing";
import {
	pairAuditLog,
	pairListDevices,
	pairRevokeDevice,
	type AuditEntry,
	type PairedDevice,
} from "@/lib/pairing-api";

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

/**
 * Phones paired with this desktop. Pairing itself happens in the relay card
 * (QR + approval); this panel lists the pinned devices and revokes them.
 */
export function PairedDevicesPanel() {
	const [devices, setDevices] = useState<PairedDevice[]>([]);
	const [loading, setLoading] = useState(false);
	const [revokingId, setRevokingId] = useState<string | null>(null);

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
							<Smartphone
								className="size-4 text-muted-foreground"
								strokeWidth={1.9}
							/>
							<h3 className="text-[14px] font-medium text-foreground">
								Dispositivos pareados
							</h3>
						</div>
						<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
							Cada celular pareado pelo relay autentica com a própria chave
							(X25519), fixada quando você aprova o pareamento. A chave privada
							nunca sai do aparelho. Revogar derruba a conexão na hora.
						</p>
					</div>
					<Badge
						variant={devices.length > 0 ? "success" : "outline"}
						className="h-7 shrink-0 px-2.5 text-[11px] font-normal"
					>
						{devices.length}
					</Badge>
				</div>

				<div className="mt-3 flex flex-wrap gap-2">
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={() => void refresh()}
					>
						<RefreshCw
							className={`size-3.5 ${loading ? "animate-spin" : ""}`}
						/>
						Atualizar
					</Button>
				</div>
			</div>

			<RelayAccessCard onPaired={() => void refresh()} />

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
												<p className="flex items-center gap-1.5 truncate font-medium text-foreground">
													{device.deviceName}
													{device.kind === "relay" ? (
														<span className="rounded border border-border/60 px-1 text-[9px] font-normal uppercase tracking-wider text-muted-foreground">
															relay
														</span>
													) : null}
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
						<RefreshCw
							className={`size-3.5 ${loading ? "animate-spin" : ""}`}
						/>
					</Button>
					<Button
						type="button"
						variant="ghost"
						size="xs"
						onClick={() => setExpanded(false)}
					>
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
			<meta.icon
				className={`mt-0.5 size-3.5 shrink-0 ${meta.tone}`}
				aria-hidden
			/>
			<div className="min-w-0 flex-1">
				<div className="flex items-center gap-2">
					<p className="font-medium text-foreground">{meta.label}</p>
					<span className="text-muted-foreground/70">
						{formatTime(entry.createdAt)}
					</span>
				</div>
				<div className="mt-0.5 flex flex-wrap gap-x-3 gap-y-0.5 text-[10px] text-muted-foreground">
					{entry.deviceId ? (
						<span className="font-mono">
							device {entry.deviceId.slice(0, 8)}…
						</span>
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
			return {
				label: "Pareamento concluído",
				icon: CheckCircle2,
				tone: "text-emerald-500",
			};
		case "revoke":
			return {
				label: "Dispositivo revogado",
				icon: XCircle,
				tone: "text-amber-500",
			};
		// The retired LAN pairing wrote these; older logs still have them.
		case "pin_failure":
			return {
				label: "PIN incorreto",
				icon: AlertTriangle,
				tone: "text-amber-500",
			};
		case "pin_locked":
			return {
				label: "Nonce travado por brute-force",
				icon: AlertTriangle,
				tone: "text-red-500",
			};
		case "relay_offer":
			return {
				label: "QR do relay gerado",
				icon: CheckCircle2,
				tone: "text-muted-foreground",
			};
		case "relay_offer_invalid":
			return {
				label: "Código do relay inválido, expirado ou já usado",
				icon: AlertTriangle,
				tone: "text-amber-500",
			};
		case "relay_pair_denied":
			return {
				label: "Pareamento pelo relay recusado",
				icon: XCircle,
				tone: "text-amber-500",
			};
		case "legacy_device_retired":
			return {
				label: "Pareamento antigo pela rede local desativado",
				icon: XCircle,
				tone: "text-muted-foreground",
			};
		case "relay_refused":
			return {
				label: "Aparelho não pareado tentou conectar pelo relay",
				icon: AlertTriangle,
				tone: "text-red-500",
			};
		case "push_token_set":
			return {
				label: "Notificações ligadas no aparelho",
				icon: CheckCircle2,
				tone: "text-muted-foreground",
			};
		case "push_token_cleared":
			return {
				label: "Notificações desligadas no aparelho",
				icon: XCircle,
				tone: "text-muted-foreground",
			};
		case "push_token_forgotten":
			return {
				label: "App removido do aparelho: notificações paradas",
				icon: XCircle,
				tone: "text-muted-foreground",
			};
		default:
			return {
				label: event,
				icon: CheckCircle2,
				tone: "text-muted-foreground",
			};
	}
}

function formatTime(iso: string): string {
	try {
		const d = new Date(iso);
		return d.toLocaleString("pt-BR", {
			dateStyle: "short",
			timeStyle: "medium",
		});
	} catch {
		return iso;
	}
}
