// In-memory objective IPC only; never changes a real conversation.
import React from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Toaster } from "sonner";
import type { SessionObjective } from "@dcc/contracts";
import { ThemeProvider, useAppearance } from "@/components/theme-provider";
import { TooltipProvider } from "@/components/ui/tooltip";
import { SessionObjectiveControl } from "@/features/sessions/SessionObjectiveControl";
import "@/i18n/config";
import "@/styles/app.css";
const win = window as any;
const scenario = new URLSearchParams(location.search).get("case");
win.objectiveCalls = [];
let failLoad = scenario === "error";
let objective: SessionObjective | null =
	scenario === "empty" || scenario === "slow"
		? null
		: {
				sessionId: "session-demo",
				intent:
					"Entregar a revisão visual do DCC com consistência entre as telas.",
				doneWhen:
					"Fluxos principais validados, documentação atualizada e layout revisado nos dois temas.",
				status: scenario === "paused" ? "paused" : "active",
				pauseReason: scenario === "paused" ? "turn_budget" : null,
				maxConsecutiveFailures: 3,
				maxTurns: 20,
				turnsUsed: 4,
				consecutiveFailures: 1,
				retries: 2,
				generation: 7,
				updatedAt: "2026-09-09T00:00:00Z",
			};
win.__TAURI_INTERNALS__ = {
	invoke: async (command: string, args: any) => {
		if (command === "plugin:app|set_app_theme") return;
		win.objectiveCalls.push({ command, args });
		switch (command) {
			case "get_session_objective":
				if (scenario === "slow")
					await new Promise((resolve) => setTimeout(resolve, 1200));
				if (failLoad) {
					failLoad = false;
					throw Error("fixture read failed");
				}
				return { objective };
			case "set_session_objective":
				await new Promise((resolve) => setTimeout(resolve, 180));
				if (scenario === "save-error")
					throw Error("Falha de gravação de demonstração.");
				objective = {
					...objective,
					...args.input.draft,
					sessionId: args.input.sessionId,
					status: "active",
					pauseReason: null,
					maxConsecutiveFailures: args.input.draft.maxConsecutiveFailures ?? 3,
					turnsUsed: 0,
					consecutiveFailures: 0,
					retries: 0,
					generation: (objective?.generation ?? 0) + 1,
					updatedAt: "2026-09-09T00:00:00Z",
				};
				return { objective };
			case "transition_session_objective":
				objective = {
					...objective!,
					status:
						args.input.transition === "complete"
							? "done"
							: args.input.transition === "pause"
								? "paused"
								: "active",
					pauseReason: args.input.transition === "pause" ? "manual" : null,
					generation: objective!.generation + 1,
				};
				return { objective };
			case "clear_session_objective":
				objective = null;
				return { objective };
			default:
				throw Error("Unexpected fixture IPC: " + command);
		}
	},
};
function Fixture() {
	const { theme, setTheme } = useAppearance();
	return (
		<>
			<button
				style={{ margin: 20 }}
				onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
			>
				Tema
			</button>
			<div style={{ position: "fixed", bottom: 20, left: 20 }}>
				<SessionObjectiveControl
					sessionId="session-demo"
					refreshKey="1"
					disabled={false}
				/>
			</div>
			<Toaster />
		</>
	);
}
createRoot(document.getElementById("root")!).render(
	<QueryClientProvider client={new QueryClient()}>
		<ThemeProvider>
			<TooltipProvider>
				<Fixture />
			</TooltipProvider>
		</ThemeProvider>
	</QueryClientProvider>,
);
