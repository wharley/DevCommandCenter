import type { ForgeCliProvider, ForgeCliStatusOutput } from "@dcc/contracts";
import { workspaceForgeCliStatus } from "./workspace-api";

export const DEFAULT_FORGE_HOSTS: Record<ForgeCliProvider, string> = {
	github: "github.com",
	gitlab: "gitlab.com",
};

function isTauriRuntime(): boolean {
	return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function getDefaultForgeHost(provider: ForgeCliProvider): string {
	return DEFAULT_FORGE_HOSTS[provider];
}

export function normalizeForgeHost(provider: ForgeCliProvider, host?: string | null): string {
	const trimmed = host?.trim();
	return trimmed && trimmed.length > 0 ? trimmed : getDefaultForgeHost(provider);
}

function fallbackForgeStatus(
	provider: ForgeCliProvider,
	host?: string | null,
	message?: string,
): ForgeCliStatusOutput {
	const hostname = normalizeForgeHost(provider, host);
	return {
		provider,
		cliName: provider === "github" ? "gh" : "glab",
		hostname,
		status: "error",
		login: null,
		logins: [],
		message: message ?? "Forge CLI is only available in the desktop runtime.",
		loginCommand: buildForgeCliDisplayCommand(provider, hostname),
	};
}

export async function getForgeCliStatus(
	provider: ForgeCliProvider,
	host?: string | null,
): Promise<ForgeCliStatusOutput> {
	if (!isTauriRuntime()) {
		return fallbackForgeStatus(provider, host);
	}

	try {
		return await workspaceForgeCliStatus({
			provider,
			host: normalizeForgeHost(provider, host),
		});
	} catch (error) {
		return fallbackForgeStatus(
			provider,
			host,
			error instanceof Error ? error.message : String(error),
		);
	}
}

function shellSingleQuote(value: string): string {
	return `'${value.replaceAll("'", `'\\''`)}'`;
}

export function buildForgeCliDisplayCommand(
	provider: ForgeCliProvider,
	host?: string | null,
): string {
	const hostname = normalizeForgeHost(provider, host);
	if (provider === "github") {
		return hostname === DEFAULT_FORGE_HOSTS.github
			? "gh auth login"
			: `gh auth login --hostname ${hostname}`;
	}
	return `glab auth login --hostname ${hostname}`;
}

export function buildForgeCliShellCommand(
	provider: ForgeCliProvider,
	host?: string | null,
): string {
	const hostname = normalizeForgeHost(provider, host);
	if (provider === "github") {
		return hostname === DEFAULT_FORGE_HOSTS.github
			? "gh auth login"
			: `gh auth login --hostname ${shellSingleQuote(hostname)}`;
	}
	return `glab auth login --hostname ${shellSingleQuote(hostname)}`;
}
