import { useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Loader2, QrCode, RefreshCw, ShieldCheck, Wifi } from "lucide-react";
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
	remoteAccessCreateOffer,
	remoteAccessPendingApprovals,
	remoteAccessRespond,
	remoteAccessSetRelay,
	remoteAccessStatus,
	type RelayApprovalRequest,
	type RelayPairingOffer,
	type RemoteAccessEvent,
	type RemoteAccessStatus,
} from "@/lib/pairing-api";

function errorMessage(error: unknown, fallback: string): string {
	if (error instanceof Error) return error.message;
	if (typeof error === "object" && error && "message" in error) {
		return String((error as { message: unknown }).message);
	}
	return fallback;
}

/**
 * Pairing through the relay: the phone reaches this desktop from any network,
 * end-to-end encrypted, with no LAN, Tailscale or account. The desktop only
 * makes an outbound connection, and only while a relay phone is paired or a
 * QR is open.
 */
export function RelayAccessCard({ onPaired }: { onPaired: () => void }) {
	const [status, setStatus] = useState<RemoteAccessStatus | null>(null);
	const [relayDraft, setRelayDraft] = useState("");
	const [saving, setSaving] = useState(false);
	const [dialogOpen, setDialogOpen] = useState(false);

	const refresh = async () => {
		try {
			const next = await remoteAccessStatus();
			setStatus(next);
			setRelayDraft(next.relayUrl ?? "");
		} catch {
			setStatus(null);
		}
	};

	useEffect(() => {
		void refresh();
		let unlisten: UnlistenFn | undefined;
		void (async () => {
			unlisten = await listen<RemoteAccessEvent>("remote-access-event", (event) => {
				if (event.payload.type === "statusChanged") void refresh();
				if (event.payload.type === "approvalResolved" && event.payload.approved) {
					onPaired();
				}
			});
		})();
		return () => unlisten?.();
	}, []);

	const saveRelay = async () => {
		setSaving(true);
		try {
			const next = await remoteAccessSetRelay(relayDraft.trim() || null);
			setStatus(next);
			toast.success(next.relayUrl ? "Relay salvo" : "Acesso pelo relay desligado");
		} catch (error) {
			toast.error(errorMessage(error, "Falha ao salvar o relay"));
		} finally {
			setSaving(false);
		}
	};

	if (!status) return null;
	const configured = Boolean(status.relayUrl);

	return (
		<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
			<div className="flex items-start justify-between gap-3">
				<div className="min-w-0">
					<div className="flex items-center gap-2">
						<Wifi className="size-4 text-muted-foreground" strokeWidth={1.9} />
						<h3 className="text-[14px] font-medium text-foreground">
							Celular de qualquer rede
						</h3>
					</div>
					<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
						O app do celular fala com este desktop por um relay, cifrado de ponta
						a ponta: o relay só repassa bytes. Sem rede local, Tailscale ou
						conta.
					</p>
				</div>
				<Badge
					variant={status.connected ? "success" : "outline"}
					className="h-7 shrink-0 px-2.5 text-[11px] font-normal"
				>
					{!configured ? "Sem relay" : status.connected ? "Conectado" : "Ocioso"}
				</Badge>
			</div>

			<label className="mt-3 block text-[12px] font-medium">
				Endereço do relay
				<div className="mt-2 flex gap-2">
					<input
						type="url"
						value={relayDraft}
						onChange={(e) => setRelayDraft(e.target.value)}
						disabled={status.relayUrlFromEnv}
						placeholder="wss://relay.exemplo.dev"
						className="min-w-0 flex-1 rounded-lg border border-border bg-background p-2 font-mono text-xs disabled:opacity-60"
					/>
					{status.relayUrlFromEnv ? null : (
						<Button
							type="button"
							variant="outline"
							size="sm"
							disabled={saving || relayDraft.trim() === (status.relayUrl ?? "")}
							onClick={() => void saveRelay()}
						>
							{saving ? <Loader2 className="size-3.5 animate-spin" /> : null}
							Salvar
						</Button>
					)}
				</div>
				<span className="mt-2 block text-[11px] font-normal leading-relaxed text-muted-foreground">
					{status.relayUrlFromEnv
						? "Definido pela variável DCC_RELAY_URL."
						: "Use um relay seu (apps/relay) ou o que o projeto publicar. Fica conectado só enquanto houver celular pareado por aqui ou um QR aberto."}
				</span>
			</label>

			<div className="mt-3 flex flex-wrap gap-2">
				<Button
					type="button"
					size="sm"
					disabled={!configured}
					onClick={() => setDialogOpen(true)}
				>
					<QrCode className="size-3.5" />
					Parear pelo relay
				</Button>
				{status.desktopFingerprint ? (
					<span className="self-center font-mono text-[11px] text-muted-foreground">
						este desktop: {status.desktopFingerprint}
					</span>
				) : null}
			</div>

			<RelayPairDialog open={dialogOpen} onClose={() => setDialogOpen(false)} />
			<RelayApprovalDialog />
		</div>
	);
}

function RelayPairDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
	const [offer, setOffer] = useState<RelayPairingOffer | null>(null);
	const [loading, setLoading] = useState(false);
	const [remaining, setRemaining] = useState(0);

	const generate = async () => {
		setLoading(true);
		try {
			setOffer(await remoteAccessCreateOffer());
		} catch (error) {
			toast.error(errorMessage(error, "Falha ao gerar o QR"));
		} finally {
			setLoading(false);
		}
	};

	useEffect(() => {
		if (open) void generate();
		else setOffer(null);
	}, [open]);

	useEffect(() => {
		if (!offer) return;
		const expires = new Date(offer.expiresAt).getTime();
		const tick = () => setRemaining(Math.max(0, Math.floor((expires - Date.now()) / 1000)));
		tick();
		const id = window.setInterval(tick, 1000);
		return () => window.clearInterval(id);
	}, [offer]);

	const expired = offer !== null && remaining <= 0;
	const minutes = Math.floor(remaining / 60);
	const seconds = String(remaining % 60).padStart(2, "0");

	return (
		<Dialog open={open} onOpenChange={(o) => !o && onClose()}>
			<DialogContent className="w-[calc(100%-2rem)] sm:max-w-sm">
				<DialogHeader>
					<DialogTitle className="text-[15px]">Parear pelo relay</DialogTitle>
					<DialogDescription className="text-[12px]">
						Escaneie com o app DCC no celular. O código vale por 5 minutos e para
						um aparelho; depois você aprova o aparelho aqui.
					</DialogDescription>
				</DialogHeader>
				<div className="flex min-h-[260px] flex-col items-center justify-center gap-3">
					{loading || !offer ? (
						<Loader2 className="size-5 animate-spin text-muted-foreground" />
					) : expired ? (
						<>
							<p className="text-[13px] font-medium text-foreground">Código expirado</p>
							<Button type="button" size="sm" onClick={() => void generate()}>
								<RefreshCw className="size-3.5" />
								Gerar outro
							</Button>
						</>
					) : (
						<>
							<RemoteAccessQr value={offer.url} size={224} />
							<p className="text-[11px] text-muted-foreground">
								expira em{" "}
								<span className="font-mono text-foreground/80">
									{minutes}:{seconds}
								</span>
							</p>
							<p className="font-mono text-[11px] text-muted-foreground">
								este desktop: {offer.desktopFingerprint}
							</p>
						</>
					)}
				</div>
				<DialogFooter>
					<Button type="button" variant="outline" onClick={onClose}>
						Fechar
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

/** "Aprovar <aparelho>?" for each phone that scanned a QR and proved it. */
function RelayApprovalDialog() {
	const [queue, setQueue] = useState<RelayApprovalRequest[]>([]);
	const [answering, setAnswering] = useState(false);

	useEffect(() => {
		void remoteAccessPendingApprovals()
			.then((res) => setQueue(res.approvals))
			.catch(() => {});
		let unlisten: UnlistenFn | undefined;
		void (async () => {
			unlisten = await listen<RemoteAccessEvent>("remote-access-event", (event) => {
				const payload = event.payload;
				if (payload.type === "approvalRequested") {
					setQueue((current) => [...current, payload.approval]);
				} else if (payload.type === "approvalResolved") {
					setQueue((current) => current.filter((item) => item.id !== payload.id));
				}
			});
		})();
		return () => unlisten?.();
	}, []);

	const current = queue[0] ?? null;

	const answer = async (approve: boolean) => {
		if (!current) return;
		setAnswering(true);
		try {
			const { delivered } = await remoteAccessRespond(current.id, approve);
			if (!delivered) toast.warning("O celular desistiu ou o pedido expirou");
			else if (approve) toast.success(`${current.deviceName} pareado`);
		} catch (error) {
			toast.error(errorMessage(error, "Falha ao responder"));
		} finally {
			setQueue((items) => items.filter((item) => item.id !== current.id));
			setAnswering(false);
		}
	};

	return (
		<Dialog open={current !== null} onOpenChange={(o) => !o && void answer(false)}>
			<DialogContent className="w-[calc(100%-2rem)] sm:max-w-sm">
				<DialogHeader>
					<DialogTitle className="text-[15px]">
						Aprovar {current?.deviceName}?
					</DialogTitle>
					<DialogDescription className="text-[12px]">
						Um celular escaneou o QR deste desktop. Aprove só se o código abaixo
						for o mesmo que ele mostra.
					</DialogDescription>
				</DialogHeader>
				<div className="flex flex-col items-center gap-2 py-3">
					<ShieldCheck className="size-5 text-muted-foreground" strokeWidth={1.8} />
					<p className="font-mono text-[22px] font-semibold tracking-[0.12em] text-foreground">
						{current?.fingerprint}
					</p>
					{current?.platform ? (
						<p className="text-[11px] text-muted-foreground">{current.platform}</p>
					) : null}
				</div>
				<DialogFooter>
					<Button
						type="button"
						variant="outline"
						disabled={answering}
						onClick={() => void answer(false)}
					>
						Recusar
					</Button>
					<Button type="button" disabled={answering} onClick={() => void answer(true)}>
						Aprovar
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
