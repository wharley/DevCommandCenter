import type { LucideIcon } from "lucide-react";
import {
	AppWindow,
	ArrowLeftRight,
	BarChart3,
	Bot,
	Cable,
	CircleUserRound,
	FileDiff,
	FileText,
	FolderGit2,
	GitMerge,
	GitPullRequest,
	GitPullRequestArrow,
	Globe,
	Layers,
	ListChecks,
	MessageSquareText,
	PanelRight,
	Search,
	Send,
	Smartphone,
	Sparkles,
	SquareTerminal,
	Target,
	Undo2,
	Workflow,
} from "lucide-react";
import {
	getCommandPaletteShortcutKeys,
	getFocusComposerShortcutKeys,
	getInspectorCodeModeShortcutKeys,
	getInspectorGitModeShortcutKeys,
	getOpenPreferredEditorShortcutKeys,
	getQuickOpenShortcutKeys,
	getToggleTerminalShortcutKeys,
} from "@/features/shortcuts/shortcut-utils";

/**
 * Help topics mirror the surfaces a person actually touches in the app.
 * Copy lives in i18n under `help.topics.<id>` so both locales stay parallel;
 * this module only fixes the order, the icon, and the shortcut for each one.
 */
export const HELP_TOPIC_IDS = [
	"workspaces",
	"composer",
	"plan",
	"objective",
	"inspector",
	"review",
	"terminal",
	"browser",
	"delegate",
	"handoff",
	"skills",
	"delivery",
	"undo",
	"mobile",
	"pullRequests",
	"spec",
	"multiProject",
	"mergeConflict",
	"mcp",
	"codeRabbit",
	"automation",
	"usage",
	"search",
	"editor",
	"account",
] as const;

export type HelpTopicId = (typeof HELP_TOPIC_IDS)[number];

export const DEFAULT_HELP_TOPIC: HelpTopicId = "workspaces";

export const HELP_TOPIC_ICONS: Record<HelpTopicId, LucideIcon> = {
	workspaces: FolderGit2,
	composer: MessageSquareText,
	plan: ListChecks,
	inspector: PanelRight,
	review: FileDiff,
	terminal: SquareTerminal,
	browser: Globe,
	delegate: Send,
	handoff: ArrowLeftRight,
	skills: Sparkles,
	delivery: GitPullRequestArrow,
	undo: Undo2,
	mobile: Smartphone,
	pullRequests: GitPullRequest,
	spec: FileText,
	objective: Target,
	multiProject: Layers,
	mergeConflict: GitMerge,
	mcp: Cable,
	codeRabbit: Bot,
	automation: Workflow,
	usage: BarChart3,
	search: Search,
	editor: AppWindow,
	account: CircleUserRound,
};

/** Shortcut shown on the topic card, when the surface has a direct one. */
export function resolveHelpTopicShortcut(topic: HelpTopicId): string[] | null {
	switch (topic) {
		case "workspaces":
			return getCommandPaletteShortcutKeys();
		case "composer":
			return getFocusComposerShortcutKeys();
		case "inspector":
			return getInspectorGitModeShortcutKeys();
		case "review":
			return getInspectorCodeModeShortcutKeys();
		case "terminal":
			return getToggleTerminalShortcutKeys();
		case "search":
			return getQuickOpenShortcutKeys();
		case "editor":
			return getOpenPreferredEditorShortcutKeys();
		default:
			return null;
	}
}

export function isHelpTopicId(value: string): value is HelpTopicId {
	return (HELP_TOPIC_IDS as readonly string[]).includes(value);
}

/** Case- and accent-insensitive match used by the in-dialog search and the palette. */
export function matchesHelpTopic(
	haystack: string,
	query: string,
): boolean {
	const tokens = normalizeHelpText(query).split(/\s+/).filter(Boolean);
	if (tokens.length === 0) return true;
	const normalized = normalizeHelpText(haystack);
	return tokens.every((token) => normalized.includes(token));
}

function normalizeHelpText(value: string): string {
	return value
		.normalize("NFD")
		.replace(/[\u0300-\u036f]/g, "")
		.toLowerCase();
}
