import type {
	CreateMcpIntegrationInput,
	McpBindingScope,
	McpIntegrationRecord,
	McpTransport,
} from "@dcc/contracts";

export type McpTransportDraft = "http" | "stdio";
export type McpScopeDraft = "session" | "project" | "global";

export type McpCredentialDraft = {
	id: string;
	name: string;
	secret: string;
};

export type McpIntegrationDraft = {
	displayName: string;
	transport: McpTransportDraft;
	url: string;
	executable: string;
	argsText: string;
	cwd: string;
	scope: McpScopeDraft;
	credentials: McpCredentialDraft[];
};

export type McpScopeContext = {
	projectId: string | null;
	sessionId: string | null;
};

export type McpIntegrationDraftError =
	| "displayName"
	| "url"
	| "executable"
	| "scope"
	| "credentialPair"
	| "credentialName"
	| "duplicateCredential";

export type BuildMcpIntegrationResult =
	| { ok: true; input: CreateMcpIntegrationInput }
	| { ok: false; error: McpIntegrationDraftError };

const ENVIRONMENT_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;
const HTTP_HEADER_NAME = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;

export function createMcpIntegrationDraft(
	context: McpScopeContext,
): McpIntegrationDraft {
	return {
		displayName: "",
		transport: "http",
		url: "",
		executable: "",
		argsText: "",
		cwd: "",
		scope: context.projectId ? "project" : "global",
		credentials: [],
	};
}

export function buildMcpIntegrationInput(
	draft: McpIntegrationDraft,
	context: McpScopeContext,
): BuildMcpIntegrationResult {
	const displayName = draft.displayName.trim();
	if (!displayName) {
		return { ok: false, error: "displayName" };
	}

	let transport: McpTransport;
	if (draft.transport === "http") {
		const url = draft.url.trim();
		try {
			const parsed = new URL(url);
			if (
				(parsed.protocol !== "http:" && parsed.protocol !== "https:") ||
				parsed.username ||
				parsed.password
			) {
				return { ok: false, error: "url" };
			}
		} catch {
			return { ok: false, error: "url" };
		}
		transport = { type: "http", url };
	} else {
		const executable = draft.executable.trim();
		if (!executable || executable.includes("\0")) {
			return { ok: false, error: "executable" };
		}
		transport = {
			type: "stdio",
			executable,
			args: draft.argsText
				.split(/\r?\n/)
				.map((argument) => argument.trim())
				.filter(Boolean),
			cwd: draft.cwd.trim() || null,
		};
	}

	let scope: McpBindingScope;
	if (draft.scope === "session") {
		if (!context.sessionId) {
			return { ok: false, error: "scope" };
		}
		scope = { type: "session", sessionId: context.sessionId };
	} else if (draft.scope === "project") {
		if (!context.projectId) {
			return { ok: false, error: "scope" };
		}
		scope = { type: "project", projectId: context.projectId };
	} else {
		scope = { type: "global" };
	}

	const credentials: NonNullable<CreateMcpIntegrationInput["credentials"]> = [];
	const names = new Set<string>();
	for (const credential of draft.credentials) {
		const name = credential.name.trim();
		const hasName = Boolean(name);
		const hasSecret = Boolean(credential.secret);
		if (!hasName && !hasSecret) {
			continue;
		}
		if (!hasName || !hasSecret) {
			return { ok: false, error: "credentialPair" };
		}

		const normalizedName =
			draft.transport === "http" ? name.toLocaleLowerCase("en-US") : name;
		const validName =
			draft.transport === "http"
				? HTTP_HEADER_NAME.test(name)
				: ENVIRONMENT_NAME.test(name);
		if (!validName) {
			return { ok: false, error: "credentialName" };
		}
		if (names.has(normalizedName)) {
			return { ok: false, error: "duplicateCredential" };
		}
		names.add(normalizedName);
		credentials.push({
			target:
				draft.transport === "http"
					? { type: "httpHeader", name }
					: { type: "environmentVariable", name },
			secret: credential.secret,
		});
	}

	return {
		ok: true,
		input: {
			displayName,
			transport,
			scope,
			credentials,
		},
	};
}

export function mcpIntegrationNeedsTrust(
	integration: McpIntegrationRecord,
): boolean {
	const { trust } = integration.definition;
	return (
		trust.decision.type === "untrusted" ||
		trust.decision.fingerprint !== trust.currentFingerprint
	);
}

export function formatMcpTransportPreview(transport: McpTransport): string {
	if (transport.type === "http") {
		return transport.url;
	}
	const parts = [
		JSON.stringify(transport.executable),
		...transport.args.map((argument) => JSON.stringify(argument)),
	];
	return parts.join(" ");
}
