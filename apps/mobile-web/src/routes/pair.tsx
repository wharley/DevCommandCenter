import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { AlertTriangle, CheckCircle2, Loader2 } from "lucide-react";
import { ApiError, apiFetch } from "@/lib/api";
import { saveSession, resolveBackendOrigin, type PairingSession } from "@/lib/session";
import { cn } from "@/lib/cn";

type CompletePairingResponse = {
	ok: boolean;
	deviceId: string;
	sessionToken?: string;
};

function detectDeviceName(): string {
	const ua = navigator.userAgent || "";
	if (/iPhone/.test(ua)) return "iPhone";
	if (/iPad/.test(ua)) return "iPad";
	const android = ua.match(/Android.*; ([^);]+)\)/);
	if (android) return android[1].trim();
	if (/Android/.test(ua)) return "Android";
	return "Mobile";
}

type PairStage =
	| { kind: "ready" }
	| { kind: "submitting" }
	| { kind: "error"; message: string }
	| { kind: "fatal"; message: string }
	| { kind: "done" };

export function PairRoute() {
	const navigate = useNavigate();
	const pinRef = useRef<HTMLInputElement | null>(null);
	const [pin, setPin] = useState("");
	const [stage, setStage] = useState<PairStage>({ kind: "ready" });

	const { backendUrl, nonce } = useMemo(() => parsePairingLink(), []);

	useEffect(() => {
		pinRef.current?.focus();
	}, []);

	if (!backendUrl || !nonce) {
		return (
			<FatalCard
				message="Link de pareamento incompleto. Gere um novo QR no app desktop."
			/>
		);
	}

	if (stage.kind === "fatal") {
		return <FatalCard message={stage.message} />;
	}

	if (stage.kind === "done") {
		return (
			<Shell>
				<section className="rounded-2xl border border-border bg-panel p-5">
					<div className="mx-auto mb-4 flex size-14 items-center justify-center rounded-full bg-accent text-[#04231b]">
						<CheckCircle2 className="size-7" strokeWidth={2.4} />
					</div>
					<h1 className="text-center text-lg font-semibold">Pareado!</h1>
					<p className="mt-1 text-center text-[13px] text-mute">
						Carregando seu painel…
					</p>
				</section>
			</Shell>
		);
	}

	const submit = async () => {
		const trimmed = pin.trim();
		if (!/^\d{6}$/.test(trimmed)) {
			setStage({ kind: "error", message: "Digite os 6 dígitos do PIN." });
			pinRef.current?.focus();
			return;
		}
		setStage({ kind: "submitting" });

		const tempSession: PairingSession = {
			deviceId: "",
			sessionToken: "",
			backendUrl,
			createdAt: new Date().toISOString(),
		};

		try {
			const body = {
				nonce,
				pin: trimmed,
				deviceName: detectDeviceName(),
			};
			const result = await apiFetch<CompletePairingResponse>(
				{ ...tempSession, sessionToken: "" },
				"/auth/pair",
				{ method: "POST", body: JSON.stringify(body) },
			);
			if (!result?.deviceId || !result?.sessionToken) {
				throw new Error("Resposta inesperada do servidor.");
			}
			await saveSession({
				deviceId: result.deviceId,
				sessionToken: result.sessionToken,
				backendUrl,
				createdAt: new Date().toISOString(),
			});
			setStage({ kind: "done" });
			window.setTimeout(() => {
				void navigate({ to: "/", replace: true });
			}, 900);
		} catch (err) {
			const message =
				err instanceof ApiError
					? err.message
					: err instanceof Error
						? err.message
						: "Falha desconhecida.";
			setStage({ kind: "error", message });
		}
	};

	const submitting = stage.kind === "submitting";
	const errorMessage = stage.kind === "error" ? stage.message : null;

	return (
		<Shell>
			<header className="px-1 pb-6">
				<h1 className="text-xl font-semibold">Parear este celular</h1>
				<p className="mt-1 text-[13px] text-mute">
					Confirme o PIN exibido no seu app desktop.
				</p>
			</header>

			<section className="rounded-2xl border border-border bg-panel p-5">
				<label
					htmlFor="pin"
					className="mb-2 block text-[11px] font-medium uppercase tracking-wider text-mute"
				>
					PIN de 6 dígitos
				</label>
				<input
					ref={pinRef}
					id="pin"
					type="tel"
					inputMode="numeric"
					pattern="[0-9]*"
					maxLength={6}
					autoComplete="one-time-code"
					placeholder="000000"
					value={pin}
					onChange={(e) => setPin(e.target.value.replace(/\D+/g, "").slice(0, 6))}
					onKeyDown={(e) => {
						if (e.key === "Enter") void submit();
					}}
					className="w-full rounded-xl border border-border bg-bg p-3.5 text-center font-mono text-[22px] font-semibold tracking-[0.4em] text-foreground outline-none focus:ring-2 focus:ring-accent"
				/>

				<button
					type="button"
					disabled={submitting}
					onClick={() => void submit()}
					className={cn(
						"mt-3 w-full rounded-xl bg-accent p-3.5 text-[15px] font-semibold text-[#04231b] transition-opacity",
						submitting && "opacity-50",
					)}
				>
					{submitting ? (
						<span className="inline-flex items-center gap-2">
							<Loader2 className="size-4 animate-spin" /> Pareando…
						</span>
					) : (
						"Parear"
					)}
				</button>

				<div className="mt-3 min-h-[18px] text-[13px]">
					{errorMessage ? (
						<span className="text-danger inline-flex items-center gap-1.5">
							<AlertTriangle className="size-3.5" />
							{errorMessage}
						</span>
					) : null}
				</div>

				<p className="mt-4 break-all text-[11px] text-mute/80">
					Backend: <code className="rounded bg-bg px-1.5 py-0.5">{backendUrl}</code>
				</p>
			</section>
		</Shell>
	);
}

function parsePairingLink(): { backendUrl: string | null; nonce: string | null } {
	const hash = window.location.hash.startsWith("#")
		? window.location.hash.slice(1)
		: "";
	const params = new URLSearchParams(hash);
	const be = params.get("be") ?? resolveBackendOrigin();
	const nonce = params.get("nonce");
	const backendUrl = be && /^https?:\/\//.test(be) ? be.replace(/\/$/, "") : null;
	return { backendUrl, nonce };
}

function Shell({ children }: { children: React.ReactNode }) {
	return (
		<main className="mx-auto flex min-h-dvh max-w-md flex-col px-5 py-8">{children}</main>
	);
}

function FatalCard({ message }: { message: string }) {
	return (
		<Shell>
			<section className="rounded-2xl border border-border bg-panel p-5">
				<h1 className="text-lg font-semibold">Não foi possível parear</h1>
				<p className="mt-2 text-[13px] text-mute">{message}</p>
				<button
					type="button"
					onClick={() => location.reload()}
					className="mt-4 w-full rounded-xl border border-border bg-transparent p-3 text-[14px]"
				>
					Tentar de novo
				</button>
			</section>
		</Shell>
	);
}
