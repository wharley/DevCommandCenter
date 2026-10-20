/**
 * Servers that register no clients (Slack) sign in through an app the user
 * creates with the vendor. DCC's callback for those apps; it must match
 * `user_app_redirect_uri(DCC_OAUTH_CALLBACK_PORT)` in dcc-infra.
 */
export const USER_APP_REDIRECT_URI = "http://localhost:46327/oauth/mcp/callback";

/**
 * Read-only user scopes of Slack's MCP server (docs.slack.dev/ai/mcp-server):
 * search, read channels, threads, files, canvases, lists and profiles.
 */
export const SLACK_READ_SCOPES = [
	"search:read.public",
	"search:read.private",
	"search:read.mpim",
	"search:read.im",
	"search:read.files",
	"search:read.users",
	"channels:history",
	"groups:history",
	"mpim:history",
	"im:history",
	"channels:read",
	"groups:read",
	"mpim:read",
	"im:read",
	"users:read",
	"users:read.email",
	"files:read",
	"canvases:read",
	"lists:read",
	"reactions:read",
	"emoji:read",
];

/**
 * Manifest for the user's internal Slack app: MCP on, PKCE on (so the
 * `localhost` redirect counts as a desktop redirect and no secret is needed)
 * and rotating tokens DCC refreshes.
 */
export function slackAppManifest() {
	return {
		display_information: {
			name: "Dev Command Center",
			description: "Lets your coding agents in Dev Command Center read Slack.",
		},
		oauth_config: {
			redirect_urls: [USER_APP_REDIRECT_URI],
			scopes: { user: SLACK_READ_SCOPES },
			pkce_enabled: true,
		},
		settings: {
			is_mcp_enabled: true,
			token_rotation_enabled: true,
			org_deploy_enabled: false,
			socket_mode_enabled: false,
		},
	};
}

/** Opens Slack's "create app from manifest" with the manifest filled in. */
export function slackCreateAppUrl(): string {
	return `https://api.slack.com/apps?new_app=1&manifest_json=${encodeURIComponent(
		JSON.stringify(slackAppManifest()),
	)}`;
}
