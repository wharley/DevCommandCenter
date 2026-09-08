import { useEffect, useState } from "react";
import { Bell } from "lucide-react";
import { apiFetch } from "@/lib/api";
import type { PairingSession } from "@/lib/session";

type PushConfig = { publicKey: string; enabled: boolean };
export function PushSettings({ session }: { session: PairingSession }) {
	const supported =
		window.isSecureContext &&
		"serviceWorker" in navigator &&
		"PushManager" in window &&
		"Notification" in window;
	const [config, setConfig] = useState<PushConfig | null>(null);
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);
	useEffect(() => {
		if (supported)
			void apiFetch<PushConfig>(session, "/api/v1/mobile/push")
				.then(setConfig)
				.catch((e) => setError(e.message));
	}, [session, supported]);
	const toggle = async () => {
		if (!supported || busy) return;
		setBusy(true);
		setError(null);
		try {
			// Ask inside the user's gesture (required by Safari).
			if (
				!config?.enabled &&
				(await Notification.requestPermission()) !== "granted"
			)
				throw new Error(
					"Permita notificações nas configurações do navegador para continuar.",
				);
			const current =
				config ?? (await apiFetch<PushConfig>(session, "/api/v1/mobile/push"));
			const registration = await navigator.serviceWorker.getRegistration("/m/");
			if (!registration?.active)
				throw new Error(
					"Instale o DCC na tela inicial e abra o aplicativo para ativar notificações.",
				);
			if (current.enabled) {
				await apiFetch(session, "/api/v1/mobile/push", { method: "DELETE" });
				await (await registration.pushManager.getSubscription())?.unsubscribe();
				setConfig({ ...current, enabled: false });
			} else {
				const key = Uint8Array.from(
					atob(current.publicKey.replace(/-/g, "+").replace(/_/g, "/")),
					(c) => c.charCodeAt(0),
				);
				const subscription =
					(await registration.pushManager.getSubscription()) ??
					(await registration.pushManager.subscribe({
						userVisibleOnly: true,
						applicationServerKey: key,
					}));
				await apiFetch(session, "/api/v1/mobile/push", {
					method: "POST",
					body: JSON.stringify(subscription.toJSON()),
				});
				setConfig({ ...current, enabled: true });
			}
		} catch (e) {
			setError(
				e instanceof Error
					? e.message
					: "Não foi possível configurar notificações.",
			);
		} finally {
			setBusy(false);
		}
	};
	return (
		<section className="mb-4 rounded-2xl border border-border bg-panel p-4">
			<h2 className="flex items-center gap-2 text-sm font-medium">
				<Bell className="size-4 text-accent" />
				Notificações
			</h2>
			<p className="mt-2 text-xs leading-relaxed text-mute">
				Receba avisos de tarefas concluídas, interrompidas e decisões pendentes,
				mesmo com o DCC fechado no celular. O computador precisa continuar
				disponível.
			</p>
			{supported ? (
				<button
					type="button"
					disabled={busy}
					onClick={() => void toggle()}
					className="mt-3 min-h-11 w-full rounded-xl border border-accent p-3 text-sm text-accent disabled:opacity-50"
				>
					{busy
						? "Configurando…"
						: config?.enabled
							? "Desativar notificações"
							: "Ativar notificações"}
				</button>
			) : (
				<p className="mt-3 text-xs text-wait">
					Abra pelo HTTPS e instale na tela inicial. No iPhone, ative por dentro
					do aplicativo instalado.
				</p>
			)}
			{error && (
				<p role="alert" className="mt-2 text-xs text-danger">
					{error}
				</p>
			)}
		</section>
	);
}
