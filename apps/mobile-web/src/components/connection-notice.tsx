import type { StreamState } from "@/lib/sseClient";

export function ConnectionNotice({
	state,
	lastSynced,
	onRetry,
}: {
	state: StreamState;
	lastSynced?: string | null;
	onRetry?: () => void;
}) {
	if (state === "connected") return null;
	const message = {
		connecting: "Conectando ao computador…",
		reconnecting:
			"Reconectando. O histórico será atualizado quando a conexão voltar.",
		offline: "Celular sem conexão. Seus rascunhos continuam salvos.",
		unauthorized:
			"Pareamento expirado ou revogado. Gere um novo QR no desktop.",
	}[state];
	return (
		<div
			role="status"
			aria-live="polite"
			className="border-b border-wait/20 bg-wait/5 px-4 py-2 text-xs text-wait"
		>
			<div className="mx-auto flex max-w-md items-center justify-between gap-3">
				<div>
					{message}
					{lastSynced && (
						<p className="mt-1 text-mute">
							Última sincronização:{" "}
							{new Date(lastSynced).toLocaleTimeString("pt-BR", {
								hour: "2-digit",
								minute: "2-digit",
							})}
						</p>
					)}
				</div>
				{onRetry && state !== "unauthorized" && (
					<button
						type="button"
						onClick={onRetry}
						className="shrink-0 rounded-lg border border-wait/30 p-2"
					>
						Atualizar
					</button>
				)}
			</div>
		</div>
	);
}
