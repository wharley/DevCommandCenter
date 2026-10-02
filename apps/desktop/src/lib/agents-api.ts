import { invoke } from "@tauri-apps/api/core";

export const AGENT_AVATAR_COLORS = ["blue", "cyan", "violet", "amber", "green", "pink"] as const;
export const AGENT_AVATAR_EYES = ["round", "smile", "visor"] as const;
export const AGENT_AVATAR_ARMS = [3, 4, 5] as const;

export type AgentAvatarColor = (typeof AGENT_AVATAR_COLORS)[number];
export type AgentAvatarEyes = (typeof AGENT_AVATAR_EYES)[number];

export type AgentAvatar = {
	color: AgentAvatarColor;
	arms: number;
	eyes: AgentAvatarEyes;
};

/** Person-authored fields of a resident agent. */
export type ResidentAgentDraft = {
	name: string;
	role: string;
	kickoffPrompt: string;
	/** What the agent asks after a turn leaves changes. Empty: it never offers. */
	offerPrompt: string;
	/** Added after the role. On a built-in agent this is the only role text the person controls. */
	extraInstructions: string;
	providerId: string | null;
	model: string | null;
	avatar: AgentAvatar;
};

/** A person-owned identity a session can run as. Global to the installation. */
export type ResidentAgent = ResidentAgentDraft & {
	id: string;
	preset: string | null;
	createdAt: string;
	updatedAt: string;
};

/** A session bound to an agent. The session still belongs to its workspace. */
export type AgentSessionBinding = {
	sessionId: string;
	agentId: string;
	workspaceId: string;
	projectId: string;
	title: string | null;
	updatedAt: string;
};

export type AgentsOverview = {
	agents: ResidentAgent[];
	sessions: AgentSessionBinding[];
};

/** The parts of the built-in reviewer the person reads, in the app language. Its role is DCC's. */
export type ReviewerPresetText = {
	name: string;
	kickoffPrompt: string;
	offerPrompt: string;
};

/** The built-in reviewer is created the first time; its visible texts follow `reviewer`. */
export function agentsOverview(reviewer: ReviewerPresetText) {
	return invoke<AgentsOverview>("agents_overview", { reviewer });
}

export function saveAgent(id: string | null, draft: ResidentAgentDraft) {
	return invoke<ResidentAgent>("agents_save", { input: { id, draft } });
}

export function bindSessionAgent(sessionId: string, agentId: string) {
	return invoke<void>("agents_bind_session", { input: { sessionId, agentId } });
}
