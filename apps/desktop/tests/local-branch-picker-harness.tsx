// UI fixture; commands are mocked and never mutate a real repository.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { LocalBranchesOutput } from "@dcc/contracts";
import { LocalBranchPicker } from "@/features/workspaces/local-branch-picker";
import { ExecutionContextRail } from "@/features/composer/ExecutionContextRail";
import "@/i18n/config";
import "@/styles/app.css";
const client = new QueryClient();
let state: LocalBranchesOutput = { currentBranch: "main", taskBranch: "main", hasConversation: false, agentRunning: false, refreshError: null,
	branches: ["main", "feature/local-branch-picker", "fix/session-history"].map((name) => ({ name, reference: `refs/heads/${name}`, remote: false })).concat([{ name: "origin/feature/new-remote", reference: "refs/remotes/origin/feature/new-remote", remote: true }]),
};
(window as any).__TAURI_INTERNALS__ = { invoke: async (command: string, args: any) => {
	if (command === "workspace_local_branches") return structuredClone(state);
	if (command === "workspace_switch_local_branch") {
		const name = args.input.newBranch ?? args.input.reference.replace(/^refs\/heads\//, "").replace(/^refs\/remotes\/origin\//, "");
		if (name.includes(" ")) throw new Error("Nome de branch inválido.");
		state = { ...state, currentBranch: name, taskBranch: name };
		if (!state.branches.some((entry) => entry.name === name)) state.branches.push({ name, reference: `refs/heads/${name}`, remote: false });
		return structuredClone(state);
	}
} };
function Fixture() {
	const [started, setStarted] = useState(false);
	const [busy, setBusy] = useState(false);
	return <main className="flex min-h-screen flex-col items-center justify-end bg-background p-8 pb-20 text-foreground">
		<section className="w-full max-w-[52rem]">
			<p className="mb-6 text-sm">{started ? "Conversa iniciada: revisar o projeto." : "Nova tarefa · DevCommandCenter"}</p>
			<div className="rounded-2xl border border-border p-4 text-sm text-muted-foreground">Descreva sua tarefa…</div>
			<ExecutionContextRail projectLabel="DevCommandCenter" baseBranch="main" currentBranch="main" isIsolatedWorkspace={false}
				localBranchControl={<LocalBranchPicker workspaceId="fixture" fallbackBranch="main" conversationStarted={started} busy={false} onBusyChange={setBusy} />} />
			<div className="mt-8 flex gap-4 text-xs">
				<button disabled={busy} onClick={() => { state.hasConversation = true; setStarted(true); void client.invalidateQueries(); }}>Enviar primeira mensagem</button>
				<button onClick={() => { state.currentBranch = "externa"; void client.invalidateQueries(); }}>Simular troca externa</button>
			</div>
		</section>
	</main>;
}
createRoot(document.getElementById("root")!).render(<QueryClientProvider client={client}><Fixture /></QueryClientProvider>);
