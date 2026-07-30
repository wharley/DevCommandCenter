import type {
	PrepareTurnOutput,
	SendTurnInput,
	WaitMcpOauthInput,
	WaitMcpOauthOutput,
} from "@dcc/contracts";

export interface McpTurnPreflightDependencies {
	prepareTurn(input: SendTurnInput): Promise<PrepareTurnOutput>;
	openAuthorizationUrl(url: string): Promise<unknown>;
	waitForOauth(input: WaitMcpOauthInput): Promise<WaitMcpOauthOutput>;
}

/**
 * Resolves every provider-reported OAuth challenge before the durable turn is
 * created. The original SendTurnInput stays in memory until preflight is ready.
 */
export async function resolveMcpTurnPreflight(
	input: SendTurnInput,
	dependencies: McpTurnPreflightDependencies,
): Promise<void> {
	const completedDefinitions = new Set<string>();

	for (;;) {
		const result = await dependencies.prepareTurn(input);
		if (result.preflight.state === "ready") return;

		const { definitionId, authorizationUrl } = result.preflight;
		if (completedDefinitions.has(definitionId)) {
			throw new Error(
				"MCP authentication did not become ready after authorization",
			);
		}
		completedDefinitions.add(definitionId);

		await dependencies.openAuthorizationUrl(authorizationUrl);
		const completion = await dependencies.waitForOauth({
			sessionId: input.sessionId,
			definitionId,
		});
		if (!completion.connected) {
			throw new Error("MCP authentication did not complete");
		}
	}
}
