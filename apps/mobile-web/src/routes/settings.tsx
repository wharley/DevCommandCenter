import { useEffect, useState } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import {
	ArrowLeft,
	Copy,
	LogOut,
	QrCode,
	Smartphone,
} from "lucide-react";
import { clearSession, loadSession, type PairingSession } from "@/lib/session";

export function SettingsRoute() {
	const navigate = useNavigate();
	const [session, setSession] = useState<PairingSession | null | undefined>(undefined);
	const [copied, setCopied] = useState(false);

	useEffect(() => {
		void loadSession().then((s) => setSession(s));
	}, []);

	const copyBackendUrl = async () => {
		if (!session?.backendUrl) return;
		try {
			await navigator.clipboard.writeText(session.backendUrl);
			setCopied(true);
			window.setTimeout(() => setCopied(false), 1500);
		} catch {
			/* ignore */
		}
	};

	const logout = async () => {
		await clearSession();
		void navigate({ to: "/", replace: true });
	};

	if (session === undefined) {
		return (
			<Shell>
				<header className="flex items-center gap-2 pb-5">
					<Link
						to="/"
						className="-ml-2 rounded-lg p-2 text-mute hover:text-foreground"
					>
						<ArrowLeft className="size-4" />
					</Link>
					<h1 className="text-xl font-semibold">Settings</h1>
				</header>
			</Shell>
		);
	}

	if (session === null) {
		return (
			<Shell>
				<header className="flex items-center gap-2 pb-5">
					<Link
						to="/"
						className="-ml-2 rounded-lg p-2 text-mute hover:text-foreground"
					>
						<ArrowLeft className="size-4" />
					</Link>
					<h1 className="text-xl font-semibold">Settings</h1>
				</header>
				<p className="rounded-2xl border border-border bg-panel p-4 text-[13px] text-mute">
					Nenhum dispositivo pareado.
				</p>
			</Shell>
		);
	}

	return (
		<Shell>
			<header className="flex items-center gap-2 pb-5">
				<Link to="/" className="-ml-2 rounded-lg p-2 text-mute hover:text-foreground">
					<ArrowLeft className="size-4" />
				</Link>
				<h1 className="text-xl font-semibold">Settings</h1>
			</header>

			<section className="rounded-2xl border border-border bg-panel p-4">
				<div className="flex items-center gap-3">
					<div className="grid size-10 place-items-center rounded-xl bg-bg">
						<Smartphone className="size-5 text-accent" strokeWidth={1.8} />
					</div>
					<div>
						<p className="text-[14px] font-medium">Dispositivo pareado</p>
						<p className="text-[11px] text-mute">
							Pareado em {formatDate(session.createdAt)}
						</p>
					</div>
				</div>

				<dl className="mt-4 space-y-3 border-t border-border/60 pt-3 text-[12px]">
					<Row label="Backend">
						<button
							type="button"
							onClick={() => void copyBackendUrl()}
							className="inline-flex items-center gap-1.5 rounded-md bg-bg px-2 py-1 font-mono text-[11px] active:opacity-60"
						>
							<span className="truncate">{session.backendUrl}</span>
							<Copy className="size-3 shrink-0 text-mute" />
						</button>
						{copied ? (
							<span className="ml-2 text-[10px] text-accent">copiado</span>
						) : null}
					</Row>
					<Row label="Device ID">
						<code className="font-mono text-[11px] text-foreground/80">
							{session.deviceId.slice(0, 8)}…
						</code>
					</Row>
				</dl>
			</section>

			<section className="mt-4 rounded-2xl border border-border bg-panel p-4">
				<h2 className="text-[14px] font-medium">Trocar de desktop</h2>
				<p className="mt-1 text-[12px] text-mute">
					Desconecta esse celular do desktop atual. Você precisará escanear um
					novo QR para parear de novo.
				</p>
				<button
					type="button"
					onClick={() => void logout()}
					className="mt-3 inline-flex w-full items-center justify-center gap-2 rounded-xl border border-border bg-bg py-2.5 text-[13px] font-medium text-foreground active:bg-muted/30"
				>
					<LogOut className="size-4" />
					Desconectar
				</button>
			</section>

			<section className="mt-4 rounded-2xl border border-border bg-panel p-4">
				<h2 className="flex items-center gap-2 text-[14px] font-medium">
					<QrCode className="size-4 text-mute" />
					Como parear outro celular
				</h2>
				<ol className="mt-2 list-decimal space-y-1 pl-5 text-[12px] leading-relaxed text-mute">
					<li>No desktop, vá em Settings → Conexões.</li>
					<li>Toque em &quot;Parear novo dispositivo&quot;.</li>
					<li>Escolha o endpoint (LAN ou Tailscale).</li>
					<li>Escaneie o QR no outro celular.</li>
				</ol>
			</section>
		</Shell>
	);
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
	return (
		<div className="flex items-center justify-between gap-3">
			<dt className="text-[11px] uppercase tracking-wider text-mute">{label}</dt>
			<dd className="min-w-0 max-w-[70%] text-right">{children}</dd>
		</div>
	);
}

function formatDate(iso: string): string {
	try {
		return new Date(iso).toLocaleString("pt-BR", {
			day: "2-digit",
			month: "short",
			hour: "2-digit",
			minute: "2-digit",
		});
	} catch {
		return iso;
	}
}

function Shell({ children }: { children: React.ReactNode }) {
	return (
		<main className="mx-auto flex min-h-dvh max-w-md flex-col px-5 py-8">{children}</main>
	);
}
