/** Default slash-command rows exposed before SDK-loaded skills arrive. */

export type SlashCommandEntry = {
	name: string;
	description: string;
	/** When `"client-action"`, parent handles insert (e.g. decorator swap). */
	source?: "builtin" | "client-action";
};

export const DEFAULT_SLASH_COMMANDS: readonly SlashCommandEntry[] = [
	{
		name: "add-dir",
		description: "Attach extra directories as context",
		source: "client-action",
	},
	{
		name: "compact",
		description: "Summarize conversation context",
		source: "builtin",
	},
	{
		name: "spec",
		description: "Draft or update this mission spec",
		source: "client-action",
	},
	{
		name: "context",
		description: "Show current context budget",
		source: "builtin",
	},
	{
		name: "commit",
		description: "Prepare a git commit message",
		source: "builtin",
	},
	{
		name: "clear",
		description: "Clear composer draft",
		source: "builtin",
	},
];
