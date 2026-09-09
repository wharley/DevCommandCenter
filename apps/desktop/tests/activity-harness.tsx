// Real assistant rendering; all messages are synthetic. No provider or user workspace access.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AssistantMessage } from "@/features/panel/message-components/AssistantMessage";
import type { WorkspaceMessageAnnotation } from "@/features/sessions/session-thread-history.logic";
import { ThemeProvider, useAppearance } from "@/components/theme-provider";
import "@/i18n/config";
import "@/styles/app.css";

const win = window as any;
win.activityIpc = [];
win.__TAURI_INTERNALS__ = {
	invoke: async (command: string) => {
		if (command === "plugin:app|set_app_theme") return;
		win.activityIpc.push(command);
		throw Error("Unexpected fixture IPC: " + command);
	},
};
const longHistory = (): WorkspaceMessageAnnotation[] =>
	Array.from({ length: 1000 }, (_, index) => ({
		type: "tool-call",
		id: `tool-${index}`,
		action: "Ler arquivo",
		command: `read src/module-${index}.ts`,
		content: `RESULT-${index}\nexport const module${index} = true;`,
	}));
const liveHistory = (): WorkspaceMessageAnnotation[] => [
	{
		type: "commentary",
		id: "start",
		content:
			"Vou conferir a autenticação e preservar o comportamento das sessões existentes.",
	},
	{
		type: "tool-call",
		id: "read",
		action: "Ler arquivo",
		file: "src/auth/session.ts",
		content: "export const persistSession = true;",
	},
	{
		type: "reasoning",
		id: "reason",
		content: "A validação precisa cobrir a restauração da sessão.",
	},
	{
		type: "commentary",
		id: "update",
		content:
			"A restauração já está ajustada. Estou verificando os testes antes de concluir.",
	},
	{
		type: "tool-call",
		id: "test",
		action: "Executar testes",
		command: "yarn test session --run",
		content: "RUN session.test.ts\n✓ restores saved sessions\n",
		streaming: true,
	},
];
function Fixture() {
	const { theme, setTheme } = useAppearance();
	const [annotations, setAnnotations] = useState(longHistory);
	const [streaming, setStreaming] = useState(false);
	const [interrupted, setInterrupted] = useState(false);
	const [key, setKey] = useState(0);
	const load = (mode: "long" | "live" | "fail" | "waiting") => {
		setKey((value) => value + 1);
		setInterrupted(false);
		setStreaming(mode === "live" || mode === "waiting");
		setAnnotations(
			mode === "long"
				? longHistory()
				: mode === "fail"
					? [
							{
								type: "tool-call",
								id: "old-failure",
								action: "Executar testes",
								content: "EXPECTED-FAILURE\nOne assertion needs attention",
								status: { type: "failed" },
							},
							...longHistory(),
						]
					: mode === "waiting"
						? [
								...liveHistory(),
								{
									type: "approval",
									id: "approval",
									title: "Executar verificação local",
									toolName: "shell",
									streaming: true,
								},
								{
									type: "native-subagent",
									id: "child",
									name: "Revisão de testes",
									status: "running",
									streaming: true,
								},
							]
						: liveHistory(),
		);
	};
	const append = () =>
		setAnnotations((current) => [
			...current,
			{
				type: "tool-call",
				id: `appended-${current.length}`,
				action: "Nova atividade",
				command: "read src/new-file.ts",
				content: "new result",
				streaming: true,
			},
		]);
	return (
		<main className="mx-auto max-w-4xl px-7 py-10 text-foreground">
			<div className="mb-8 flex flex-wrap gap-4 text-xs text-muted-foreground">
				<button onClick={() => load("long")}>Histórico longo</button>
				<button onClick={() => load("live")}>Execução ativa</button>
				<button onClick={() => load("fail")}>Falha antiga</button>
				<button onClick={() => load("waiting")}>Pedir aprovação</button>
				<button onClick={append}>Adicionar evento</button>
				<button
					onClick={() => {
						setStreaming(false);
						setAnnotations((current) =>
							current.map((item) => ({ ...item, streaming: false })),
						);
					}}
				>
					Concluir
				</button>
				<button
					onClick={() => {
						setStreaming(false);
						setInterrupted(true);
					}}
				>
					Interromper
				</button>
				<button onClick={() => setTheme(theme === "dark" ? "light" : "dark")}>
					Trocar tema
				</button>
			</div>
			<div className="mb-8 ml-auto max-w-md rounded-2xl border border-border bg-sidebar px-5 py-4 text-sm">
				Revise a restauração das sessões e valide os testes.
			</div>
			<AssistantMessage
				key={key}
				content={
					streaming
						? ""
						: "A restauração das sessões foi revisada. Os resultados e as verificações estão disponíveis no histórico acima."
				}
				annotations={annotations}
				streaming={streaming}
				status={
					interrupted
						? {
								type: "incomplete",
								reason: "Execução interrompida na fixture.",
							}
						: undefined
				}
				modelId="Example Agent"
				onFork={() => {
					win.activityForks = (win.activityForks ?? 0) + 1;
				}}
				sessionId="fixture-session"
			/>
		</main>
	);
}
createRoot(document.getElementById("root")!).render(
	<ThemeProvider defaultTheme="dark">
		<TooltipProvider>
			<Fixture />
		</TooltipProvider>
	</ThemeProvider>,
);
