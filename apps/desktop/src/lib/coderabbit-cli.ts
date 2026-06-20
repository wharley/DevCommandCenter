import type { WorkspaceCodeRabbitCliStatusOutput } from "@dcc/contracts";
import { openTerminalAtPath } from "./shell-api";
import { workspaceCodeRabbitCliStatus } from "./workspace-api";

function isTauriRuntime(): boolean {
	return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const CODERABBIT_LOGIN_COMMAND = "cr auth login";

function fallbackCodeRabbitStatus(message?: string): WorkspaceCodeRabbitCliStatusOutput {
	return {
		cliName: "cr",
		cliPath: null,
		installed: false,
		status: "unavailable",
		version: null,
		message: message ?? "CodeRabbit CLI is only available in the desktop runtime.",
		loginCommand: CODERABBIT_LOGIN_COMMAND,
		auth: null,
	};
}

export async function getCodeRabbitCliStatus(options?: {
	workspaceRoot?: string | null;
	cliPath?: string | null;
	includeAuthStatus?: boolean;
}): Promise<WorkspaceCodeRabbitCliStatusOutput> {
	if (!isTauriRuntime()) {
		return fallbackCodeRabbitStatus();
	}

	try {
		return await workspaceCodeRabbitCliStatus({
			workspaceRoot: options?.workspaceRoot?.trim() || null,
			cliPath: options?.cliPath?.trim() || null,
			includeAuthStatus: options?.includeAuthStatus ?? true,
		});
	} catch (error) {
		return fallbackCodeRabbitStatus(
			error instanceof Error ? error.message : String(error),
		);
	}
}

export function buildCodeRabbitLoginShellCommand(): string {
	return CODERABBIT_LOGIN_COMMAND;
}

export async function openCodeRabbitCliAuthTerminal() {
	return openTerminalAtPath("~", { command: buildCodeRabbitLoginShellCommand() });
}
