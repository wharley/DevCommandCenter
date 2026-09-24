// Development-only UI fixture: never accesses real projects or providers.
import React from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ThemeProvider } from "@/components/theme-provider";
import { TooltipProvider } from "@/components/ui/tooltip";
import { QuickComposer } from "@/features/quick-composer/QuickComposer";
import { FALLBACK_PROVIDER_CATALOG } from "@/lib/fallback-provider-catalog";
import type { QuickLaunch } from "@/features/quick-composer/api";
import "@/i18n/config";
import "@/styles/app.css";

const params = new URLSearchParams(location.search);
const calls: string[] = [];
let launch: QuickLaunch | null = null;
let counter = 0;
const callbacks = new Map<number, (event: unknown) => void>();
const repositories = ["Dev Command Center", "Orbit"].map((name, index) => ({
	id: `repo-${index}`,
	projectId: `project-${index}`,
	name,
	displayName: name,
	rootPath: `/fixture/${index ? "orbit" : "dcc"}`,
	baseBranch: "main",
	icon: null,
	color: null,
	pinnedAt: null,
	remote: null,
	remoteUrl: null,
	forgeProvider: null,
	forgeLogin: null,
	createdAt: "",
	updatedAt: "",
}));
Object.assign(window, {
	__TAURI_INTERNALS__: {
		metadata: {
			currentWindow: { label: "quick-composer" },
			currentWebview: { label: "quick-composer" },
		},
		transformCallback: (callback: (event: unknown) => void) => {
			callbacks.set(++counter, callback);
			return counter;
		},
		unregisterCallback: (id: number) => callbacks.delete(id),
		convertFileSrc: (path: string) => path,
		invoke: async (command: string, args?: Record<string, any>) => {
			calls.push(command);
			switch (command) {
				case "list_repositories":
					return { repositories: params.has("empty") ? [] : repositories };
				case "list_workspaces":
					return {
						workspaces:
							launch?.workspaceId && !params.has("removed")
								? [
										{
											id: launch.workspaceId,
											state: params.get("taskState") ?? "ready",
										},
									]
								: [],
					};
				case "list_providers":
					return {
						catalog: {
							providers: FALLBACK_PROVIDER_CATALOG.providers.map((p) => ({
								...p,
								health: "Healthy",
								enabled: true,
							})),
						},
					};
				case "quick_composer_status":
					return {
						supported: true,
						shortcut: "Super+Shift+KeyC",
						shortcutError: false,
						launch,
					};
				case "quick_composer_begin":
					launch = args!.launch;
					return launch;
				case "quick_composer_checkpoint":
					launch = args!.launch;
					return launch;
				case "create_workspace_for_repo":
					return { workspace: { id: `task-${args!.input.isolationMode}` } };
				case "start_thread":
					if (params.has("failure"))
						throw new Error("Fixture: provider unavailable");
					return { session: { id: "session" } };
				case "prepare_turn":
					return { preflight: { state: "ready" } };
				case "send_turn":
					return {};
				case "appshots_status":
					return { supported: false, targets: [] };
				case "quick_composer_hide":
					document.body.dataset.hidden = "true";
					return;
				case "quick_composer_open_main":
					document.body.dataset.openedTask = launch?.workspaceId ?? "main";
					return;
				case "plugin:event|listen":
					return ++counter;
				case "plugin:event|unlisten":
					return;
				case "provider_account_usage":
					return { usage: null };
				default:
					return null;
			}
		},
	},
	__TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
});

function Fixture() {
	return (
		<ThemeProvider>
			<TooltipProvider>
				<QueryClientProvider
					client={
						new QueryClient({ defaultOptions: { queries: { retry: false } } })
					}
				>
					<div className="mx-auto max-w-[720px]">
						<QuickComposer />
					</div>
				</QueryClientProvider>
			</TooltipProvider>
		</ThemeProvider>
	);
}
createRoot(document.getElementById("root")!).render(<Fixture />);
