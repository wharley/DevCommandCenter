import { invoke } from "@tauri-apps/api/core";
import { MCP_METHODS } from "@dcc/contracts";
import type {
	ActivateMcpDefinitionInput,
	ActivateMcpIntegrationOutput,
	CreateMcpIntegrationInput,
	CreateMcpIntegrationOutput,
	DisableMcpIntegrationInput,
	DisableMcpIntegrationOutput,
	DisconnectMcpOauthInput,
	DisconnectMcpOauthOutput,
	ListMcpIntegrationsOutput,
	RemoveMcpIntegrationInput,
	RemoveMcpIntegrationOutput,
	SetMcpToolPolicyInput,
	SetMcpToolPolicyOutput,
} from "@dcc/contracts";

function isTauriRuntime(): boolean {
	return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function requireTauriRuntime(): void {
	if (!isTauriRuntime()) {
		throw new Error("MCP integration management requires the desktop runtime");
	}
}

export async function listMcpIntegrations(): Promise<ListMcpIntegrationsOutput> {
	if (!isTauriRuntime()) {
		return { integrations: [] };
	}
	return invoke<ListMcpIntegrationsOutput>(MCP_METHODS.listMcpIntegrations);
}

export function createMcpIntegration(input: CreateMcpIntegrationInput) {
	requireTauriRuntime();
	return invoke<CreateMcpIntegrationOutput>(MCP_METHODS.createMcpIntegration, {
		input,
	});
}

export function activateMcpIntegration(input: ActivateMcpDefinitionInput) {
	requireTauriRuntime();
	return invoke<ActivateMcpIntegrationOutput>(
		MCP_METHODS.activateMcpIntegration,
		{ input },
	);
}

export function disableMcpIntegration(input: DisableMcpIntegrationInput) {
	requireTauriRuntime();
	return invoke<DisableMcpIntegrationOutput>(
		MCP_METHODS.disableMcpIntegration,
		{ input },
	);
}

export function removeMcpIntegration(input: RemoveMcpIntegrationInput) {
	requireTauriRuntime();
	return invoke<RemoveMcpIntegrationOutput>(
		MCP_METHODS.removeMcpIntegration,
		{ input },
	);
}

export function disconnectMcpOauth(input: DisconnectMcpOauthInput) {
	requireTauriRuntime();
	return invoke<DisconnectMcpOauthOutput>(MCP_METHODS.disconnectMcpOauth, {
		input,
	});
}

export function setMcpToolPolicy(input: SetMcpToolPolicyInput) {
	requireTauriRuntime();
	return invoke<SetMcpToolPolicyOutput>(MCP_METHODS.setMcpToolPolicy, {
		input,
	});
}
