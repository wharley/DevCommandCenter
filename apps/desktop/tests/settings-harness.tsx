// Real settings components with synthetic IPC. Never connects accounts or modifies user projects.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ThemeProvider, useAppearance } from "@/components/theme-provider";
import { TooltipProvider } from "@/components/ui/tooltip";
import { SettingsDialog } from "@/features/settings/SettingsDialog";
import "@/i18n/config";
import "@/styles/app.css";

const win = window as any;
win.settingsCalls = [];
win.unexpectedSettingsCalls = [];
win.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
win.__TAURI_INTERNALS__ = {
	transformCallback: () => 1,
	unregisterCallback: () => {},
	invoke: async (command: string, args: any) => {
		win.settingsCalls.push(command);
		if (command === "plugin:app|set_app_theme") return;
		if (command === "plugin:event|listen") return 1;
		if (command === "plugin:event|unlisten") return;
		if (command === "pair_list_devices") return { devices: [] };
		if (command === "pair_get_endpoints") return { endpoints: [] };
		if (command === "list_mcp_integrations") return { integrations: [] };
		if (
			[
				"workspace_forge_cli_accounts",
				"workspace_forge_cli_status",
				"workspace_forge_cli_hosts",
			].includes(command)
		)
			return {
				provider: args.input.provider,
				cliName: "gh",
				hostname: "github.com",
				status: "error",
				accounts: [],
				logins: [],
				hosts: [],
				login: null,
				selectedLogin: null,
				message: "Synthetic disconnected account",
				loginCommand: "gh auth login",
			};
		if (command === "workspace_coderabbit_cli_status")
			return {
				cliName: "cr",
				cliPath: null,
				installed: false,
				status: "unavailable",
				version: null,
				message: "Synthetic disconnected integration",
				loginCommand: "cr auth login",
				auth: null,
			};
		win.unexpectedSettingsCalls.push(command);
		throw new Error(`Unexpected settings fixture IPC: ${command}`);
	},
};
const client = new QueryClient({
	defaultOptions: { queries: { retry: false } },
});
function Fixture() {
	const [open, setOpen] = useState(false);
	const { theme, setTheme, density, setDensity } = useAppearance();
	return (
		<main className="min-h-screen bg-background p-12 text-foreground">
			<button onClick={() => setOpen(true)}>Abrir configurações</button>
			<SettingsDialog
				open={open}
				onOpenChange={setOpen}
				onOpenShortcuts={() => {
					win.settingsShortcuts = (win.settingsShortcuts ?? 0) + 1;
				}}
				theme={theme}
				onThemeChange={setTheme}
				density={density}
				onDensityChange={setDensity}
				providerCatalog={null}
				selectedProviderId={null}
				onSelectProvider={() => {}}
				selectedModelId={null}
				onSelectModel={() => {}}
				providerRuntimeSettings={{}}
				onChangeProviderRuntime={() => {}}
				onClearProviderRuntime={() => {}}
				appVersion="0.8.0"
				onCheckForUpdate={() => {
					win.settingsUpdateChecks = (win.settingsUpdateChecks ?? 0) + 1;
				}}
				workspaceName="Example project"
				workspaceRoot={null}
				projectId={null}
				sessionId={null}
				sessionProviderId={null}
				sessionCreatedAt={null}
			/>
		</main>
	);
}
createRoot(document.getElementById("root")!).render(
	<QueryClientProvider client={client}>
		<ThemeProvider defaultTheme="dark">
			<TooltipProvider>
				<Fixture />
			</TooltipProvider>
		</ThemeProvider>
	</QueryClientProvider>,
);
