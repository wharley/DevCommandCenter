export type TerminalScopeKind = "worktree" | "project";

export type TerminalScopeTarget = {
	kind: TerminalScopeKind;
	label: string;
	scopeKey: string;
	cwd: string | null;
	projectLabel: string;
	branchLabel: string | null;
	protected: boolean;
	disabledReason?: string | null;
};

export type OpenTerminalRequest = {
	scope: TerminalScopeKind;
	terminalId?: string;
	createNew?: boolean;
};
