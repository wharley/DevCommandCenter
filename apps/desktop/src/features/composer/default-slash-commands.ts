/** Provider-independent slash commands owned and executed by DCC. */

import type { Capabilities } from "@dcc/contracts";

export type SlashCommandEntry = {
	name: string;
	description: string;
	/** When `"client-action"`, parent handles insert (e.g. decorator swap). */
	source?: "builtin" | "client-action";
	/** Listed only when the selected provider declares this capability. */
	requiresCapability?: keyof Pick<
		Capabilities,
		"supportsCompactionCommand" | "supportsNativeGoal"
	>;
};

export const DEFAULT_SLASH_COMMANDS: readonly SlashCommandEntry[] = [
	{
		name: "spec",
		description: "Draft or update this mission spec",
		source: "client-action",
	},
	{
		name: "clear",
		description: "Clear composer draft",
		source: "client-action",
	},
	{
		name: "compact",
		description: "Compact the provider context and re-anchor",
		source: "client-action",
		requiresCapability: "supportsCompactionCommand",
	},
	{
		// The provider's own command: inserted for the person to finish
		// (`/goal <condition>`) and sent as written.
		name: "goal",
		description: "Set a goal the agent keeps working toward until it is done",
		requiresCapability: "supportsNativeGoal",
	},
];

/**
 * Projects the command list through the provider capabilities: a command
 * bound to a capability the runtime does not declare is not offered at all,
 * instead of being sent as plain text and failing quietly.
 */
export function availableSlashCommands(
	commands: readonly SlashCommandEntry[],
	capabilities: Partial<Capabilities> | null | undefined,
): SlashCommandEntry[] {
	return commands.filter(
		(command) =>
			!command.requiresCapability || capabilities?.[command.requiresCapability] === true,
	);
}
