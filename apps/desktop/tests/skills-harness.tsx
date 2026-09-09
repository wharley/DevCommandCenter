// All skill reads/writes are in-memory fixtures. No local skills or agents are changed.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { Toaster } from "sonner";
import { ThemeProvider, useAppearance } from "@/components/theme-provider";
import { TooltipProvider } from "@/components/ui/tooltip";
import { SkillsDialog } from "@/features/skills/SkillsDialog";
import type { SkillRecord, SkillContextDetection } from "@/lib/skills-api";
import "@/i18n/config";
import "@/styles/app.css";
const win = window as any;
const scenario = new URLSearchParams(location.search).get("case");
win.skillCalls = [];
win.skillsChanged = 0;
let failLoad = scenario === "error";
let records: SkillRecord[] =
	scenario === "empty"
		? []
		: [
				{
					name: "review-pr",
					description:
						"Revise mudanças com foco em comportamento, testes e compatibilidade. Explique os achados com contexto suficiente para agir.",
					body: "# Revisar código\n\nConfira o diff e os testes relevantes.",
					targetAgents: ["claude", "codex"],
					disableModelInvocation: false,
					scope: "project",
				},
				{
					name: "test-workflows",
					description:
						"Verifique os fluxos principais antes de concluir uma tarefa.",
					body: "# Validar\n\nTeste o caminho principal e os estados de erro.",
					targetAgents: ["gemini"],
					disableModelInvocation: true,
					scope: "project",
				},
				{
					name: "document-project-conventions-and-review-long-context",
					description:
						"Registre as decisões e convenções que ajudam a manter consistência entre contribuições.",
					body: "# Documentação\n\nDocumente as decisões.",
					targetAgents: ["cursor", "agents"],
					disableModelInvocation: false,
					scope: "project",
				},
			];
const detected: SkillContextDetection[] = [
	{
		id: "source",
		kind: "dcc_source",
		title: "DCC skills",
		relativePath: ".devcommandcenter/skills/",
		rootKind: "project_root",
		count: 3,
		managedCount: 3,
		externalCount: 0,
		hasDccBlock: false,
	},
	{
		id: "external",
		kind: "codex_skills",
		title: "Codex skills",
		relativePath:
			".agents/skills/project-specific-review-conventions-and-additional-context/",
		rootKind: "target_root",
		count: 4,
		managedCount: 1,
		externalCount: 3,
		hasDccBlock: false,
	},
	{
		id: "instructions",
		kind: "instructions_file",
		title: "AGENTS.md",
		relativePath: "AGENTS.md",
		rootKind: "project_root",
		count: 1,
		managedCount: 0,
		externalCount: 1,
		hasDccBlock: false,
	},
];
win.__TAURI_INTERNALS__ = {
	invoke: async (command: string, args: any) => {
		if (command === "plugin:app|set_app_theme") return;
		win.skillCalls.push({ command, args });
		switch (command) {
			case "skills_list":
				if (failLoad) {
					failLoad = false;
					throw "Não foi possível ler o catálogo de demonstração.";
				}
				return records.map((record) => ({ ...record }));
			case "skills_detect_context":
				return scenario === "empty" ? [] : detected;
			case "skills_save":
				await new Promise((resolve) => setTimeout(resolve, 120));
				records = [
					...records.filter((record) => record.name !== args.skill.name),
					args.skill,
				];
				return;
			case "skills_delete":
				records = records.filter((record) => record.name !== args.name);
				return;
			case "skills_compile":
				if (scenario === "compile-fail")
					throw "Falha de compilação de demonstração.";
				return;
			default:
				throw Error("Unexpected fixture IPC: " + command);
		}
	},
};
function Fixture() {
	const [open, setOpen] = useState(false);
	const { theme, setTheme } = useAppearance();
	return (
		<div style={{ padding: 24 }}>
			<button onClick={() => setOpen(true)}>Abrir skills</button>
			<button
				style={{ marginLeft: 20 }}
				onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
			>
				Tema
			</button>
			<SkillsDialog
				open={open}
				onOpenChange={setOpen}
				projectRoot={scenario === "none" ? null : "/fixture/studio"}
				workspaceId={scenario === "readonly" ? null : "workspace-demo"}
				onSkillsChanged={() => {
					win.skillsChanged++;
				}}
			/>
			<Toaster />
		</div>
	);
}
createRoot(document.getElementById("root")!).render(
	<ThemeProvider>
		<TooltipProvider>
			<Fixture />
		</TooltipProvider>
	</ThemeProvider>,
);
