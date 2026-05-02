import { invoke } from "@tauri-apps/api/core";
import { WORKSPACE_METHODS } from "@dcc/contracts";
import type {
	GithubCliStatusInput,
	GithubCliStatusOutput,
} from "@dcc/contracts";
import { openTerminalAtPath } from "./shell-api";

function isTauriRuntime(): boolean {
	return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function getGithubCliStatus(): Promise<GithubCliStatusOutput> {
	if (!isTauriRuntime()) {
		return {
			cliName: "gh",
			hostname: "github.com",
			status: "error",
			login: null,
			message: "GitHub CLI is only available in the desktop runtime.",
			loginCommand: "gh auth login",
		};
	}

	try {
		return await invoke<GithubCliStatusOutput>(
			WORKSPACE_METHODS.workspaceGithubCliStatus,
			{ input: {} satisfies GithubCliStatusInput },
		);
	} catch (error) {
		return {
			cliName: "gh",
			hostname: "github.com",
			status: "error",
			login: null,
			message: error instanceof Error ? error.message : String(error),
			loginCommand: "gh auth login",
		};
	}
}

export async function openGithubCliAuthTerminal() {
	return openTerminalAtPath("~", { command: "gh auth login" });
}
