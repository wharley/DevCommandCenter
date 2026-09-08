type InstallEvent = Event & {
	prompt(): Promise<void>;
	userChoice: Promise<{ outcome: string }>;
};
let installPrompt: InstallEvent | null = null;
export const getInstallPrompt = () => installPrompt;
export const clearInstallPrompt = () => {
	installPrompt = null;
};
window.addEventListener("beforeinstallprompt", (event) => {
	event.preventDefault();
	installPrompt = event as InstallEvent;
	window.dispatchEvent(new Event("dcc-install-ready"));
});

export async function registerPwa(): Promise<void> {
	if (
		!import.meta.env.PROD ||
		!window.isSecureContext ||
		!("serviceWorker" in navigator)
	)
		return;
	try {
		const registration = await navigator.serviceWorker.register("/m/sw.js", {
			scope: "/m/",
			updateViaCache: "none",
		});
		const announce = () => {
			if (registration.waiting)
				window.dispatchEvent(
					new CustomEvent("dcc-update-ready", { detail: registration.waiting }),
				);
		};
		announce();
		registration.addEventListener("updatefound", () =>
			registration.installing?.addEventListener("statechange", announce),
		);
		document.addEventListener("visibilitychange", () => {
			if (document.visibilityState === "visible")
				void registration.update().catch(() => {});
		});
	} catch (error) {
		console.warn("Não foi possível preparar o acesso offline do DCC.", error);
	}
}
