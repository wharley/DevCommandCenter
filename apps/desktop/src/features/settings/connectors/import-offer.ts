import type {
	McpIntegrationRecord,
	NativeMcpApp,
	NativeMcpServerSummary,
} from "@dcc/contracts";
import { findPresetForUrl, sameConnectorUrl } from "./catalog";
import { findIntegrationForUrl } from "./connector-model";

/**
 * The import offer shows once per server (UX rule 8): servers already
 * offered, imported or declined are remembered by URL in this browser.
 */
const OFFERED_KEY = "dcc.connectors.importOffered";

export const NATIVE_APP_NAMES: Record<NativeMcpApp, string> = {
	claudeCode: "Claude Code",
	codex: "Codex",
	cursor: "Cursor",
};

export type ImportCandidate = NativeMcpServerSummary & { displayName: string };

/** Servers worth offering: not in DCC yet, not offered before, one per URL. */
export function importCandidates(
	found: NativeMcpServerSummary[],
	integrations: McpIntegrationRecord[],
	offered: string[],
): ImportCandidate[] {
	const candidates: ImportCandidate[] = [];
	for (const server of found) {
		if (
			findIntegrationForUrl(integrations, server.url) ||
			offered.some((url) => sameConnectorUrl(url, server.url)) ||
			candidates.some((candidate) => sameConnectorUrl(candidate.url, server.url))
		) {
			continue;
		}
		candidates.push({ ...server, displayName: importDisplayName(server) });
	}
	return candidates;
}

/** The catalog name when the URL is a preset, else the server's own name. */
export function importDisplayName(server: NativeMcpServerSummary): string {
	const preset = findPresetForUrl(server.url);
	if (preset) return preset.name;
	const name = server.name
		.replace(/([a-z0-9])([A-Z])/g, "$1 $2")
		.replace(/[-_]+/g, " ")
		.trim();
	return name ? name.charAt(0).toLocaleUpperCase() + name.slice(1) : server.url;
}

export function readOfferedUrls(): string[] {
	try {
		const stored = JSON.parse(window.localStorage.getItem(OFFERED_KEY) ?? "[]");
		return Array.isArray(stored) ? stored.filter((url) => typeof url === "string") : [];
	} catch {
		return [];
	}
}

export function rememberOffered(urls: string[]) {
	try {
		const merged = [...new Set([...readOfferedUrls(), ...urls])].slice(-500);
		window.localStorage.setItem(OFFERED_KEY, JSON.stringify(merged));
	} catch {
		// Private storage off: the offer may show again, nothing breaks.
	}
}

/** "ClickUp and Linear" in the UI language. */
export function joinNames(names: string[], locale: string): string {
	try {
		return new Intl.ListFormat(locale, { style: "long", type: "conjunction" }).format(names);
	} catch {
		return names.join(", ");
	}
}
