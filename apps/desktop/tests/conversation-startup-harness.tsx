// Exercise real timeline transitions with synthetic events and no provider calls.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import type { CoreEvent } from "@dcc/contracts";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ActiveThreadViewport } from "@/features/panel/ActiveThreadViewport";
import { projectWorkspaceMessages } from "@/features/panel/thread-projection";
import "@/i18n/config";
import "@/styles/app.css";

const prompt = "Revise a autenticação do projeto e preserve as sessões existentes.";
const sessionId = "b912712a-58d9-4e3d-bf13-a75de3ba350f";
const started: CoreEvent = { sessionStarted: { session_id: sessionId, workspace_id: "ws", project_id: "project", provider_id: "codex" } };
const accepted: CoreEvent = { sessionTurnStarted: { session_id: sessionId, turn_id: "turn-1", prompt } };

function Fixture() {
	const { i18n } = useTranslation("common");
	const [stage, setStage] = useState("creating");
	const creating = stage === "creating";
	const running = ["accepted", "activity"].includes(stage);
	const pending = !["idle", "activity"].includes(stage) ? prompt : null;
	const events = creating ? [] : running ? [started, accepted] : [started];
	if (stage === "activity") events.push({ sessionTurnDelta: { session_id: sessionId, turn_id: "turn-1", content: "Vou conferir a autenticação e a restauração das sessões." } });
	const messages = projectWorkspaceMessages([], events, creating ? null : sessionId, pending);
	return (
		<main className="mx-auto flex h-screen max-w-3xl flex-col text-foreground">
			<nav className="flex flex-wrap gap-3 border-b border-border p-4 text-xs">
				{["idle", "creating", "hydrating", "sending", "accepted", "activity"].map(value => <button key={value} onClick={() => setStage(value)}>{value}</button>)}
				<button onClick={() => void i18n.changeLanguage(i18n.language === "en" ? "pt-BR" : "en")}>Idioma</button>
				<button onClick={() => document.documentElement.classList.toggle("dark")}>Tema</button>
			</nav>
			<ActiveThreadViewport
				messages={messages} hasLoaded={stage !== "hydrating"} isEmpty={creating}
				workspaceName="Autenticação" sessionState="active" lastTurnState={running ? "running" : null}
				pendingPrompt={pending} workspacePath={null} sessionId={creating ? null : sessionId}
				planMessageId={null} planApproved={false} planReadOnly={false} onSelectSession={() => {}}
			/>
		</main>
	);
}

createRoot(document.getElementById("root")!).render(<TooltipProvider><Fixture /></TooltipProvider>);
