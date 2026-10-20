import { describe, expect, it } from "vitest";
import type { McpConnectionCheck, McpIntegrationRecord } from "@dcc/contracts";
import en from "@/i18n/locales/en/common.json";
import ptBR from "@/i18n/locales/pt-BR/common.json";
import {
	CONNECTOR_PRESETS,
	findPresetForUrl,
	sameConnectorUrl,
	visibleConnectorPresets,
} from "./catalog";
import {
	deriveConnectorView,
	findIntegrationForUrl,
	isValidScopeList,
	MAX_LOGO_BYTES,
	readLogoFile,
	sortConnectorViews,
} from "./connector-model";
import { importCandidates, importDisplayName, joinNames } from "./import-offer";
import {
	SLACK_READ_SCOPES,
	slackAppManifest,
	slackCreateAppUrl,
	USER_APP_REDIRECT_URI,
} from "./user-app";
import {
	defaultConnectorName,
	parseCustomConnectorUrl,
} from "./AddCustomConnectorDialog";

const NO_HINTS = {
	readOnlyHint: null,
	destructiveHint: null,
	idempotentHint: null,
	openWorldHint: null,
};

function record(
	overrides: {
		name?: string;
		url?: string;
		enabled?: boolean;
		trusted?: boolean;
		signedIn?: boolean;
		check?: Partial<McpConnectionCheck> | null;
	} = {},
): McpIntegrationRecord {
	const fingerprint = "a".repeat(64);
	return {
		definition: {
			id: `mcp-${overrides.name ?? "clickup"}`,
			displayName: overrides.name ?? "ClickUp",
			transport: { type: "http", url: overrides.url ?? "https://mcp.clickup.com/mcp" },
			secretRefs: [],
			enabled: overrides.enabled ?? true,
			ownership: { type: "dccManaged" },
			trust: {
				currentFingerprint: fingerprint,
				decision:
					overrides.trusted === false
						? { type: "untrusted" }
						: { type: "trusted", fingerprint, trustedAt: "2026-10-18T00:00:00Z" },
			},
			createdAt: "2026-10-18T00:00:00Z",
			updatedAt: "2026-10-18T00:00:00Z",
		},
		bindings: [],
		toolPolicies: [],
		credentialCount: 0,
		oauthProviderIds: overrides.signedIn ? ["dcc"] : [],
		scopes: null,
		logo: null,
		connectionCheck: overrides.check
			? {
					definitionId: `mcp-${overrides.name ?? "clickup"}`,
					state: "connected",
					tools: [],
					message: null,
					checkedAt: "2026-10-18T00:00:00Z",
					...overrides.check,
				}
			: null,
	} as McpIntegrationRecord;
}

describe("connector catalog", () => {
	it("has copy for every preset in both languages", () => {
		for (const preset of CONNECTOR_PRESETS) {
			expect(ptBR.settings.connectors.catalog).toHaveProperty(preset.id);
			expect(en.settings.connectors.catalog).toHaveProperty(preset.id);
		}
	});

	it("keeps unverified presets out of the shelves", () => {
		const visible = visibleConnectorPresets().map((preset) => preset.id);
		expect(visible).toEqual(["clickup", "linear", "notion", "github", "sentry", "slack"]);
	});

	it("matches URLs ignoring a trailing slash and host case", () => {
		expect(sameConnectorUrl("https://MCP.Linear.app/mcp/", "https://mcp.linear.app/mcp")).toBe(
			true,
		);
		expect(sameConnectorUrl("https://mcp.linear.app/mcp", "https://mcp.linear.app/sse")).toBe(
			false,
		);
		expect(findPresetForUrl("https://api.githubcopilot.com/mcp")?.id).toBe("github");
	});
});

describe("connector view", () => {
	it("trusts the last check over the sign-in", () => {
		expect(deriveConnectorView(record({ signedIn: true })).status).toBe("connected");
		expect(
			deriveConnectorView(record({ signedIn: true, check: { state: "needsAuth" } })).status,
		).toBe("needsAuth");
		expect(deriveConnectorView(record({ check: { state: "failed" } })).status).toBe("failed");
		expect(deriveConnectorView(record()).status).toBe("unchecked");
	});

	it("asks to reconnect connectors signed in only by a provider", () => {
		const legacy = { ...record(), oauthProviderIds: ["claude_code"] } as McpIntegrationRecord;
		expect(deriveConnectorView(legacy).status).toBe("needsAuth");
		const moved = { ...record(), oauthProviderIds: ["claude_code", "dcc"] } as McpIntegrationRecord;
		expect(deriveConnectorView(moved).status).toBe("connected");
	});

	it("reports disabled or untrusted connectors as off", () => {
		expect(deriveConnectorView(record({ enabled: false, signedIn: true })).status).toBe("off");
		expect(deriveConnectorView(record({ trusted: false })).status).toBe("off");
	});

	it("counts tools and picks the preset logo", () => {
		const view = deriveConnectorView(
			record({
				check: {
					tools: [
						{ name: "get_task", annotations: NO_HINTS },
						{ name: "create_task", annotations: NO_HINTS },
					],
				},
			}),
		);
		expect(view.toolCount).toBe(2);
		expect(view.logoId).toBe("clickup");
	});

	it("lists connectors that need the user first", () => {
		const sorted = sortConnectorViews([
			deriveConnectorView(record({ name: "Alpha", url: "https://a.example.com/mcp" })),
			deriveConnectorView(
				record({ name: "Zulu", url: "https://z.example.com/mcp", check: { state: "needsAuth" } }),
			),
		]);
		expect(sorted.map((view) => view.name)).toEqual(["Zulu", "Alpha"]);
	});

	it("finds an existing integration by URL", () => {
		const integrations = [record()];
		expect(findIntegrationForUrl(integrations, "https://mcp.clickup.com/mcp/")).toBe(
			integrations[0],
		);
		expect(findIntegrationForUrl(integrations, "https://mcp.linear.app/mcp")).toBeUndefined();
	});
});

describe("custom connector URL", () => {
	it("accepts https and loopback http only", () => {
		expect(parseCustomConnectorUrl("https://mcp.example.com/mcp")?.toString()).toBe(
			"https://mcp.example.com/mcp",
		);
		expect(parseCustomConnectorUrl("http://localhost:3000/mcp")).not.toBeNull();
		expect(parseCustomConnectorUrl("http://mcp.example.com/mcp")).toBeNull();
		expect(parseCustomConnectorUrl("https://user:pw@mcp.example.com/mcp")).toBeNull();
		expect(parseCustomConnectorUrl("https://intranet/mcp")).toBeNull();
		expect(parseCustomConnectorUrl("not a url")).toBeNull();
	});

	it("drops the fragment and suggests a name from the host", () => {
		const url = parseCustomConnectorUrl("https://mcp.supabase.com/mcp#x")!;
		expect(url.toString()).toBe("https://mcp.supabase.com/mcp");
		expect(defaultConnectorName(url)).toBe("Supabase");
		expect(defaultConnectorName(new URL("https://app.zedy.com.br/mcp"))).toBe("Zedy");
		expect(defaultConnectorName(new URL("https://localhost:3000/mcp"))).toBe("Localhost");
		expect(
			defaultConnectorName(
				new URL("https://zedy-git-dcc-analisar-api-zedy-checkout.vercel.app/mcp/internal"),
			),
		).toBe("Zedy");
	});
});

describe("custom connector details", () => {
	it("accepts OAuth scope lists only", () => {
		expect(isValidScopeList("support:read reports:read")).toBe(true);
		expect(isValidScopeList("  catalog:read   store:read ")).toBe(true);
		expect(isValidScopeList("")).toBe(false);
		expect(isValidScopeList('bad"scope')).toBe(false);
		expect(isValidScopeList("a\\b")).toBe(false);
	});

	it("reads small images and refuses other files", async () => {
		const png = new File([new Uint8Array([137, 80, 78, 71])], "logo.png", { type: "image/png" });
		await expect(readLogoFile(png)).resolves.toMatch(/^data:image\/png;base64,/);
		const html = new File(["<h1>"], "logo.html", { type: "text/html" });
		await expect(readLogoFile(html)).rejects.toThrow();
		const huge = new File([new Uint8Array(MAX_LOGO_BYTES + 1)], "big.png", { type: "image/png" });
		await expect(readLogoFile(huge)).rejects.toThrow();
	});

	it("shows a saved logo only for connectors outside the catalog", () => {
		const custom = {
			...record({ name: "Zedy", url: "https://zedy.example.com/mcp" }),
			logo: "data:image/png;base64,AAAA",
		} as McpIntegrationRecord;
		expect(deriveConnectorView(custom).logoUri).toBe("data:image/png;base64,AAAA");
		const preset = { ...record(), logo: "data:image/png;base64,AAAA" } as McpIntegrationRecord;
		expect(deriveConnectorView(preset).logoUri).toBeNull();
	});
});

describe("Slack app", () => {
	it("registers DCC's localhost callback with PKCE and MCP on", () => {
		const manifest = slackAppManifest();
		expect(USER_APP_REDIRECT_URI).toBe("http://localhost:46327/oauth/mcp/callback");
		expect(manifest.oauth_config.redirect_urls).toEqual([USER_APP_REDIRECT_URI]);
		expect(manifest.oauth_config.pkce_enabled).toBe(true);
		expect(manifest.settings.is_mcp_enabled).toBe(true);
	});

	it("asks only for read scopes and requests the same at sign-in", () => {
		expect(SLACK_READ_SCOPES.every((scope) => !scope.endsWith(":write"))).toBe(true);
		expect(findPresetForUrl("https://mcp.slack.com/mcp")?.scopes).toBe(
			SLACK_READ_SCOPES.join(" "),
		);
		expect(isValidScopeList(SLACK_READ_SCOPES.join(" "))).toBe(true);
	});

	it("opens Slack with the manifest filled in", () => {
		const url = new URL(slackCreateAppUrl());
		expect(url.origin).toBe("https://api.slack.com");
		expect(url.searchParams.get("new_app")).toBe("1");
		expect(JSON.parse(url.searchParams.get("manifest_json")!)).toEqual(slackAppManifest());
	});
});

describe("import offer", () => {
	const server = (url: string, name = "server", app: "claudeCode" | "codex" | "cursor" = "claudeCode") => ({
		app,
		name,
		url,
		hasHeaders: false,
	});

	it("offers each new server once, under its catalog name", () => {
		const candidates = importCandidates(
			[
				server("https://mcp.clickup.com/mcp/", "clickup"),
				server("https://mcp.linear.app/mcp", "linear", "codex"),
				server("https://mcp.linear.app/mcp/", "linear-copy", "cursor"),
				server("https://mcp.notion.com/mcp", "notion"),
				server("https://internal.example.com/mcp", "my_tools"),
			],
			[record()],
			["https://mcp.notion.com/mcp"],
		);
		expect(candidates.map((candidate) => candidate.displayName)).toEqual([
			"Linear",
			"My tools",
		]);
	});

	it("turns config keys into readable names", () => {
		expect(importDisplayName(server("https://developers.openai.com/mcp", "openaiDeveloperDocs"))).toBe(
			"Openai Developer Docs",
		);
		expect(importDisplayName(server("https://mcp.mercadopago.com/mcp", "mercadopago-mcp"))).toBe(
			"Mercadopago mcp",
		);
	});

	it("joins names in the UI language", () => {
		expect(joinNames(["ClickUp", "Linear"], "pt-BR")).toBe("ClickUp e Linear");
		expect(joinNames(["ClickUp", "Linear"], "en")).toBe("ClickUp and Linear");
	});
});
