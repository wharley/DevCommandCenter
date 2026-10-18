/**
 * Curated connectors, data only. How each one signs in is detected by
 * probing the server, not declared here, except for servers that need a
 * token or the user's own OAuth app. A preset ships only after a real
 * sign-in was verified (docs/PLANO_CONECTORES_MCP.md, D7).
 */

export type ConnectorCategory = "popular" | "developer" | "productivity";

export type ConnectorPreset = {
	id: string;
	name: string;
	category: ConnectorCategory;
	url: string;
	docsUrl: string;
	/** Key into connector-logos; null falls back to the initial. */
	logoId: string | null;
	/** Servers without client registration: a personal access token. */
	auth?: "token";
	tokenHelpUrl?: string;
	/** Hidden until the server is live or its sign-in path is verified. */
	hidden?: boolean;
};

export const CONNECTOR_CATEGORIES: ConnectorCategory[] = [
	"popular",
	"developer",
	"productivity",
];

export const CONNECTOR_PRESETS: ConnectorPreset[] = [
	{
		id: "clickup",
		name: "ClickUp",
		category: "popular",
		url: "https://mcp.clickup.com/mcp",
		docsUrl: "https://developer.clickup.com/docs/connect-an-ai-assistant-to-clickups-mcp-server",
		logoId: "clickup",
	},
	{
		id: "linear",
		name: "Linear",
		category: "popular",
		url: "https://mcp.linear.app/mcp",
		docsUrl: "https://linear.app/docs/mcp",
		logoId: "linear",
	},
	{
		id: "notion",
		name: "Notion",
		category: "popular",
		url: "https://mcp.notion.com/mcp",
		docsUrl: "https://developers.notion.com/docs/mcp",
		logoId: "notion",
	},
	{
		id: "github",
		name: "GitHub",
		category: "developer",
		url: "https://api.githubcopilot.com/mcp/",
		docsUrl: "https://github.com/github/github-mcp-server",
		logoId: "github",
		auth: "token",
		tokenHelpUrl: "https://github.com/settings/personal-access-tokens/new",
	},
	{
		id: "sentry",
		name: "Sentry",
		category: "developer",
		url: "https://mcp.sentry.dev/mcp",
		docsUrl: "https://docs.sentry.io/product/sentry-mcp/",
		logoId: "sentry",
	},
	{
		// Needs the user's own Slack app (no client registration); the guided
		// setup ships with the custom-connector flow (F5).
		id: "slack",
		name: "Slack",
		category: "productivity",
		url: "https://mcp.slack.com/mcp",
		docsUrl: "https://api.slack.com/apps",
		logoId: "slack",
		hidden: true,
	},
	{
		// Production answers 403 until the server is deployed.
		id: "zedy",
		name: "Zedy",
		category: "productivity",
		url: "https://app.zedy.com.br/mcp",
		docsUrl: "https://app.zedy.com.br/docs",
		logoId: null,
		hidden: true,
	},
];

export function visibleConnectorPresets(): ConnectorPreset[] {
	return CONNECTOR_PRESETS.filter((preset) => !preset.hidden);
}

/** Compares server URLs ignoring a trailing slash, case of the host and a fragment. */
export function sameConnectorUrl(left: string, right: string): boolean {
	const normalize = (value: string) => {
		try {
			const url = new URL(value.trim());
			url.hash = "";
			return `${url.protocol}//${url.host.toLowerCase()}${url.pathname.replace(/\/+$/, "")}${url.search}`;
		} catch {
			return value.trim().replace(/\/+$/, "");
		}
	};
	return normalize(left) === normalize(right);
}

export function findPresetForUrl(url: string): ConnectorPreset | undefined {
	return CONNECTOR_PRESETS.find((preset) => sameConnectorUrl(preset.url, url));
}
