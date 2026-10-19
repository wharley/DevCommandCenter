import type {
	McpConnectFailure,
	McpConnectionCheck,
	McpIntegrationRecord,
} from "@dcc/contracts";
import {
	activateMcpIntegration,
	createMcpIntegration,
	fetchMcpConnectorLogo,
	setMcpConnectorLogo,
	setMcpConnectorScopes,
	startMcpConnect,
	testMcpIntegration,
	waitMcpConnect,
} from "@/lib/mcp-api";
import { mcpIntegrationNeedsTrust } from "../mcp-integration-form";
import {
	findPresetForUrl,
	sameConnectorUrl,
	type ConnectorPreset,
} from "./catalog";

/** Grant owner for sign-ins DCC performs itself (crates/dcc-tauri mcp_connect). */
export const DCC_GRANT_OWNER = "dcc";

/**
 * What Settings shows for a connector. Only two states matter to the user —
 * working or needs reconnecting; the rest are quiet details.
 */
export type ConnectorStatus =
	| "connected"
	| "needsAuth"
	| "failed"
	| "off"
	| "unchecked";

export type ConnectorView = {
	integration: McpIntegrationRecord;
	preset: ConnectorPreset | undefined;
	name: string;
	logoId: string | null;
	/** The user's or the site's logo (`data:` URI), for non-catalog connectors. */
	logoUri: string | null;
	status: ConnectorStatus;
	toolCount: number;
	message: string | null;
	isRemote: boolean;
	signedIn: boolean;
};

export function connectorUrl(integration: McpIntegrationRecord): string | null {
	const { transport } = integration.definition;
	return transport.type === "http" ? transport.url : null;
}

export function isSignedIn(integration: McpIntegrationRecord): boolean {
	return integration.oauthProviderIds.includes(DCC_GRANT_OWNER);
}

export function deriveConnectorView(
	integration: McpIntegrationRecord,
): ConnectorView {
	const url = connectorUrl(integration);
	const preset = url ? findPresetForUrl(url) : undefined;
	const check = integration.connectionCheck ?? null;
	const enabled =
		integration.definition.enabled && !mcpIntegrationNeedsTrust(integration);
	// Signed in only through a provider's own in-turn login (the old
	// mcp-remote path): reconnecting moves it to DCC's sign-in.
	const legacySignIn =
		url !== null &&
		!isSignedIn(integration) &&
		integration.oauthProviderIds.length > 0;
	let status: ConnectorStatus;
	if (!enabled) {
		status = "off";
	} else if (legacySignIn || check?.state === "needsAuth") {
		status = "needsAuth";
	} else if (check?.state === "failed") {
		status = "failed";
	} else if (check?.state === "connected") {
		status = "connected";
	} else {
		status = isSignedIn(integration) ? "connected" : "unchecked";
	}
	return {
		integration,
		preset,
		name: integration.definition.displayName,
		logoId: preset?.logoId ?? null,
		logoUri: preset ? null : (integration.logo ?? null),
		status,
		toolCount: check?.tools?.length ?? 0,
		message: check?.message ?? null,
		isRemote: url !== null,
		signedIn: isSignedIn(integration),
	};
}

/** Connectors that need the user first, then by name. */
export function sortConnectorViews(views: ConnectorView[]): ConnectorView[] {
	const rank = (status: ConnectorStatus) => (status === "needsAuth" ? 0 : 1);
	return [...views].sort(
		(left, right) =>
			rank(left.status) - rank(right.status) ||
			left.name.localeCompare(right.name),
	);
}

export function findIntegrationForUrl(
	integrations: McpIntegrationRecord[],
	url: string,
): McpIntegrationRecord | undefined {
	return integrations.find((integration) => {
		const existing = connectorUrl(integration);
		return existing !== null && sameConnectorUrl(existing, url);
	});
}

export type ConnectorTarget = {
	name: string;
	url: string;
	preset?: ConnectorPreset;
	/** Custom connectors: scopes to request instead of the server's. */
	scopes?: string;
	/** Custom connectors: an uploaded logo (`data:` URI). */
	logo?: string;
};

/**
 * Returns an enabled, trusted, global integration for `target`, reusing an
 * existing one with the same URL. Clicking Connect is the user's consent, so
 * a connector needs no separate trust review.
 */
export async function ensureConnectorIntegration(
	target: ConnectorTarget,
	integrations: McpIntegrationRecord[],
	token?: string,
): Promise<McpIntegrationRecord> {
	let integration = token
		? undefined
		: findIntegrationForUrl(integrations, target.url);
	if (!integration) {
		const created = await createMcpIntegration({
			displayName: target.name,
			transport: { type: "http", url: target.url },
			scope: { type: "global" },
			credentials: token
				? [
						{
							target: { type: "httpHeader", name: "Authorization" },
							secret: `Bearer ${token}`,
						},
					]
				: [],
		});
		integration = created.integration;
		await applyNewConnectorDetails(integration, target);
	}
	if (!integration.definition.enabled || mcpIntegrationNeedsTrust(integration)) {
		const activated = await activateMcpIntegration({
			definitionId: integration.definition.id,
			expectedFingerprint: integration.definition.trust.currentFingerprint,
		});
		integration = activated.integration;
	}
	return integration;
}

/**
 * Saves what the user chose when adding a custom connector. Without an
 * uploaded logo, the site's icon is looked up in the background.
 */
async function applyNewConnectorDetails(
	integration: McpIntegrationRecord,
	target: ConnectorTarget,
) {
	const definitionId = integration.definition.id;
	if (target.scopes?.trim()) {
		await setMcpConnectorScopes({ definitionId, scopes: target.scopes });
	}
	if (target.logo) {
		await setMcpConnectorLogo({ definitionId, logo: target.logo });
	} else if (!target.preset) {
		void fetchMcpConnectorLogo({ definitionId }).catch(() => undefined);
	}
}

/** RFC 6749 scope tokens separated by spaces; mirrors the backend check. */
export function isValidScopeList(value: string): boolean {
	const scopes = value.trim().split(/\s+/).filter(Boolean);
	return (
		scopes.length > 0 &&
		scopes.join(" ").length <= 1024 &&
		scopes.every((scope) => /^[\x21\x23-\x5B\x5D-\x7E]+$/.test(scope))
	);
}

const LOGO_TYPES = ["image/png", "image/jpeg", "image/gif", "image/webp", "image/svg+xml"];
export const MAX_LOGO_BYTES = 256 * 1024;

/** Reads an image the user picked into a `data:` URI, or rejects it. */
export function readLogoFile(file: File): Promise<string> {
	if (!LOGO_TYPES.includes(file.type) || file.size > MAX_LOGO_BYTES) {
		return Promise.reject(new Error("invalid-logo"));
	}
	return new Promise((resolve, reject) => {
		const reader = new FileReader();
		reader.onload = () =>
			typeof reader.result === "string" ? resolve(reader.result) : reject(new Error("invalid-logo"));
		reader.onerror = () => reject(new Error("invalid-logo"));
		reader.readAsDataURL(file);
	});
}

export type ConnectOutcome =
	| { kind: "connected"; check: McpConnectionCheck | null }
	| { kind: "failed"; reason: McpConnectFailure; message: string };

/**
 * Runs DCC's sign-in for a remote integration. `openBrowser` receives the
 * authorization URL; `onLoginStarted` gets the id needed to cancel.
 */
export async function connectIntegration(
	integration: McpIntegrationRecord,
	{
		openBrowser,
		onLoginStarted,
		clientId,
		clientSecret,
	}: {
		openBrowser: (url: string) => Promise<unknown>;
		onLoginStarted?: (loginId: string) => void;
		clientId?: string;
		clientSecret?: string;
	},
): Promise<ConnectOutcome> {
	const definitionId = integration.definition.id;
	const started = await startMcpConnect({
		definitionId,
		clientId: clientId ?? null,
		clientSecret: clientSecret ?? null,
	});
	if (started.kind === "failed") {
		return started;
	}
	if (started.kind === "noAuthRequired") {
		const { check } = await testMcpIntegration({ definitionId });
		return { kind: "connected", check };
	}
	onLoginStarted?.(started.loginId);
	await openBrowser(started.authorizationUrl);
	return waitMcpConnect({ loginId: started.loginId });
}
