import { invoke } from "@tauri-apps/api/core";
import { MCP_METHODS } from "@dcc/contracts";
import type {
	ActivateMcpDefinitionInput,
	CancelMcpConnectOutput,
	DisconnectMcpConnectorInput,
	DisconnectMcpConnectorOutput,
	McpConnectLoginInput,
	SetMcpProjectEnabledInput,
	SetMcpProjectEnabledOutput,
	StartMcpConnectInput,
	StartMcpConnectOutput,
	TestMcpIntegrationInput,
	TestMcpIntegrationOutput,
	WaitMcpConnectOutput,
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

/** Starts DCC's own sign-in for a remote connector. */
export function startMcpConnect(input: StartMcpConnectInput) {
	requireTauriRuntime();
	return invoke<StartMcpConnectOutput>(MCP_METHODS.startMcpConnect, { input });
}

/** Resolves when the browser returns, the sign-in is cancelled, or it times out. */
export function waitMcpConnect(input: McpConnectLoginInput) {
	requireTauriRuntime();
	return invoke<WaitMcpConnectOutput>(MCP_METHODS.waitMcpConnect, { input });
}

export function cancelMcpConnect(input: McpConnectLoginInput) {
	requireTauriRuntime();
	return invoke<CancelMcpConnectOutput>(MCP_METHODS.cancelMcpConnect, {
		input,
	});
}

/** Revokes DCC's sign-in with the vendor and forgets it. */
export function disconnectMcpConnector(input: DisconnectMcpConnectorInput) {
	requireTauriRuntime();
	return invoke<DisconnectMcpConnectorOutput>(
		MCP_METHODS.disconnectMcpConnector,
		{ input },
	);
}

export function testMcpIntegration(input: TestMcpIntegrationInput) {
	requireTauriRuntime();
	return invoke<TestMcpIntegrationOutput>(MCP_METHODS.testMcpIntegration, {
		input,
	});
}

/** Switches an integration off (or back on) in one project. */
export function setMcpProjectEnabled(input: SetMcpProjectEnabledInput) {
	requireTauriRuntime();
	return invoke<SetMcpProjectEnabledOutput>(MCP_METHODS.setMcpProjectEnabled, {
		input,
	});
}
