import { useEffect, useState } from "react";
import { Download, RefreshCw } from "lucide-react";

import { getInstallPrompt, clearInstallPrompt } from "@/lib/pwa";

export function InstallApp() {
	const [prompt, setPrompt] = useState(getInstallPrompt());
	const [waiting, setWaiting] = useState<ServiceWorker | null>(null);
	const [installed, setInstalled] = useState(
		window.matchMedia("(display-mode: standalone)").matches ||
			Boolean((navigator as Navigator & { standalone?: boolean }).standalone),
	);
	useEffect(() => {
		const ready = () => setPrompt(getInstallPrompt());
		const done = () => {
			setInstalled(true);
			setPrompt(null);
			clearInstallPrompt();
		};
		const update = (e: Event) =>
			setWaiting((e as CustomEvent<ServiceWorker>).detail);
		window.addEventListener("dcc-install-ready", ready);
		window.addEventListener("appinstalled", done);
		window.addEventListener("dcc-update-ready", update);
		if ("serviceWorker" in navigator)
			void navigator.serviceWorker.getRegistration("/m/").then((r) => {
				if (r?.waiting) setWaiting(r.waiting);
			});
		return () => {
			window.removeEventListener("dcc-install-ready", ready);
			window.removeEventListener("appinstalled", done);
			window.removeEventListener("dcc-update-ready", update);
		};
	}, []);
	return (
		<section className="mb-4 rounded-2xl border border-border bg-panel p-4">
			<h2 className="flex items-center gap-2 text-sm font-medium">
				<Download className="size-4 text-accent" />
				DCC na tela inicial
			</h2>
			<p className="mt-2 text-xs leading-relaxed text-mute">
				{installed
					? "Aplicativo instalado. Os rascunhos e o histórico recente ficam disponíveis neste celular."
					: !window.isSecureContext
						? "Para instalar, abra o DCC pelo endereço HTTPS do Tailscale. O acesso por HTTP continua funcionando no navegador."
						: "No iPhone: Compartilhar → Adicionar à Tela de Início. No Android: menu do navegador → Instalar aplicativo."}
			</p>
			{prompt && !installed && (
				<button
					type="button"
					className="mt-3 min-h-11 w-full rounded-xl bg-accent p-3 text-sm font-semibold text-accent-ink"
					onClick={async () => {
						await prompt.prompt();
						await prompt.userChoice;
						clearInstallPrompt();
						setPrompt(null);
					}}
				>
					Instalar DCC
				</button>
			)}
			{waiting && (
				<button
					type="button"
					className="mt-3 flex w-full items-center justify-center gap-2 rounded-xl border border-accent p-3 text-sm text-accent"
					onClick={() => {
						navigator.serviceWorker.addEventListener(
							"controllerchange",
							() => window.location.reload(),
							{ once: true },
						);
						waiting.postMessage({ type: "ACTIVATE_UPDATE" });
					}}
				>
					<RefreshCw className="size-4" />
					Atualizar aplicativo
				</button>
			)}
		</section>
	);
}
