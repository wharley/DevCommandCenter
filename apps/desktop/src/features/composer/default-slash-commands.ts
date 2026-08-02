/** Provider-independent slash commands owned and executed by DCC. */

export type SlashCommandEntry = {
	name: string;
	description: string;
	/** When `"client-action"`, parent handles insert (e.g. decorator swap). */
	source?: "builtin" | "client-action";
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
];
