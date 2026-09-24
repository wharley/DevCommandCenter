// Isolated UI fixture. Never reads the user's database or launches providers.
import React from "react";
import { createRoot } from "react-dom/client";
import { ThemeProvider } from "@/components/theme-provider";
import { MenuBar } from "@/features/menu-bar/MenuBar";
import type { MenuBarTask } from "@/features/menu-bar/api";
import i18n from "@/i18n/config";
import "@/styles/app.css";
const params = new URLSearchParams(location.search);
localStorage.setItem("dcc-theme", params.get("theme") ?? "dark");
localStorage.setItem("dcc.ui.locale", params.get("lang") ?? "pt-BR");
void i18n.changeLanguage(params.get("lang") ?? "pt-BR");
const statuses: MenuBarTask["status"][] = ["permission", "running", "running", "completed", "aborted"];
const titles = ["Atualizar autenticação do GitHub", "Implementar barra de menus", "Revisar testes do composer", "Corrigir navegação entre tarefas", "Investigar lentidão no build"];
let callbackId = 0;
Object.assign(window, {
	__TAURI_INTERNALS__: {
		metadata: { currentWindow: { label: "menu-bar" }, currentWebview: { label: "menu-bar" } },
		transformCallback: () => ++callbackId,
		unregisterCallback: () => {},
		invoke: async (command: string, args?: Record<string, unknown>) => {
			if (command === "plugin:event|listen") return ++callbackId;
			if (command === "menu_bar_visible") return true;
			if (command === "menu_bar_snapshot") {
				if (params.has("error")) throw new Error("Fixture unavailable");
				return { tasks: params.has("empty") ? [] : statuses.map((status, index) => ({
					workspaceId: `w${index}`, sessionId: `s${index}`, status, title: titles[index], project: index === 2 ? "Orbit" : "Dev Command Center", updatedAt: "2026-09-24T14:00:00Z",
				})), metrics: { cpuPercent: 12.4, memoryBytes: 820 * 1024 ** 2, processCount: 8 } };
			}
			if (command.startsWith("menu_bar_")) document.body.dataset.action = JSON.stringify({ command, args });
			return null;
		},
	},
	__TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
});
createRoot(document.getElementById("root")!).render(<ThemeProvider><MenuBar /></ThemeProvider>);
