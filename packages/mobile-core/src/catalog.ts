/**
 * "Nova tarefa" from the phone: what the desktop offers (`GET
 * /api/v1/mobile/catalog`), the defaults the desktop would pick, and the body
 * of `POST /api/v1/mobile/tasks`. Also the project list behind the Workspaces
 * tab, which must show every project — not only the ones that already have a
 * workspace (a project added on the desktop has none until its first task).
 */
import { pathBasename, splitPrompt } from "./attachments.ts";
import { t } from "./labels.ts";
import type { Comb, SessionSearchResult } from "./triage.ts";

export type CatalogModel = {
	id: string;
	label: string;
	description?: string;
	recommended?: boolean;
};

/** `health` is `"Healthy"`, `{ Degraded: { reason } }` or `{ Unhealthy: { reason } }`. */
export type CatalogProvider = {
	id: string;
	label: string;
	description?: string;
	models: CatalogModel[];
	health?: unknown;
	enabled?: boolean;
	stable?: boolean;
	/** `approvalPolicies`: the access levels the agent enforces itself. */
	/** `vision: false`: images go as plain file references (desktop rule). */
	capabilities?: { approvalPolicies?: string[]; vision?: boolean };
};

export type CatalogRepository = {
	id: string;
	projectId: string;
	name: string;
	displayName?: string | null;
	icon?: string | null;
	color?: string | null;
	logo?: string | null;
	rootPath: string;
	baseBranch: string;
};

export type CatalogWorkspace = {
	id: string;
	projectId: string;
	name: string | null;
	rootPath: string;
	baseBranch: string;
	worktreePath: string | null;
	state: string;
	updatedAt?: string;
};

export type MobileCatalog = {
	providers: CatalogProvider[];
	repositories: CatalogRepository[];
	workspaces: CatalogWorkspace[];
};

export function repositoryLabel(repository: CatalogRepository): string {
	return repository.displayName?.trim() || repository.name;
}

/** Why a provider can't take a task right now, or null when it can. */
export function providerUnavailableReason(provider: CatalogProvider): string | null {
	if (provider.enabled === false) return "Desligado no computador";
	const health = provider.health;
	if (health && typeof health === "object" && "Unhealthy" in health) {
		const reason = (health as { Unhealthy?: { reason?: string } }).Unhealthy?.reason;
		return reason?.trim() || "Indisponível no computador";
	}
	return null;
}

/** Same pick as the desktop: the remembered one, else the first stable, else the first usable. */
export function defaultProviderId(
	providers: CatalogProvider[],
	remembered: string | null = null,
): string | null {
	const usable = providers.filter((p) => providerUnavailableReason(p) === null);
	if (remembered && usable.some((p) => p.id === remembered)) return remembered;
	return (usable.find((p) => p.stable) ?? usable[0])?.id ?? null;
}

/** Recommended model, else the first; null when the provider lists none. */
export function defaultModelId(provider: CatalogProvider | null | undefined): string | null {
	if (!provider) return null;
	return (provider.models.find((m) => m.recommended) ?? provider.models[0])?.id ?? null;
}

/** A title for the task from the request's first line (no LLM: deterministic and editable later). */
export function taskTitle(prompt: string, max = 60): string {
	const line =
		prompt
			.split("\n")
			.map((l) => l.trim())
			.find(Boolean) ?? "";
	const flat = line.replace(/\s+/g, " ");
	if (!flat) return "Nova tarefa";
	return flat.length > max ? `${flat.slice(0, max - 1).trimEnd()}…` : flat;
}

/**
 * The access levels a person picks, in the desktop's order
 * (`ComposerApprovalPolicyMenu`). `read_only` stays internal: DCC uses it for
 * delegated reviews.
 */
export const ACCESS_POLICIES = ["ask", "auto", "full_access"] as const;
export type AccessPolicy = (typeof ACCESS_POLICIES)[number];

export function isAccessPolicy(value: unknown): value is AccessPolicy {
	return (ACCESS_POLICIES as readonly unknown[]).includes(value);
}

/** What this agent lets the person choose; empty when it manages access itself. */
export function accessPolicies(provider: CatalogProvider | null | undefined): AccessPolicy[] {
	const supported = provider?.capabilities?.approvalPolicies ?? [];
	return ACCESS_POLICIES.filter((policy) => supported.includes(policy));
}

/**
 * The desktop's pick (`loadApprovalPolicy`): the preferred level when the agent
 * offers it, else "Aprovar por mim", else the first offered; null when none.
 */
export function pickAccessPolicy(
	available: readonly AccessPolicy[],
	...preferred: Array<AccessPolicy | null | undefined>
): AccessPolicy | null {
	for (const policy of preferred) if (policy && available.includes(policy)) return policy;
	if (available.includes("auto")) return "auto";
	return available[0] ?? null;
}

export function accessLabel(policy: AccessPolicy): string {
	return t(`composer.approval.options.${policy}.label`);
}

export function accessDescription(policy: AccessPolicy): string {
	return t(`composer.approval.options.${policy}.description`);
}

export type TaskDraft = {
	requestId: string;
	repositoryId: string;
	prompt: string;
	providerId: string;
	model: string | null;
	/** Checked by default, like the desktop: the task gets its own worktree. */
	worktree: boolean;
	/** Null when the agent manages access itself. */
	access?: AccessPolicy | null;
};

/** Body of `POST /api/v1/mobile/tasks`. Every task gets a new workspace, as on the desktop. */
function titleSource(prompt: string): string {
	const written = splitPrompt(prompt);
	return written.text || written.attachments.map(pathBasename).join(", ");
}

export function taskRequestBody(draft: TaskDraft) {
	return {
		requestId: draft.requestId,
		repositoryId: draft.repositoryId,
		// `@/path` attachments are not part of the title.
		title: taskTitle(titleSource(draft.prompt)),
		prompt: draft.prompt.trim(),
		providerId: draft.providerId,
		model: draft.model,
		isolationMode: draft.worktree ? "protectedWorktree" : "localDirect",
		...(draft.access ? { approvalPolicy: draft.access } : {}),
	} as const;
}

/** `GET /api/v1/mobile/tasks/:requestId`. `sessionId` shows up before the first turn starts. */
export type TaskProgress = {
	state: "creating" | "started" | "failed";
	workspaceId?: string;
	sessionId?: string;
	error?: string;
};

export type ProjectGroup = {
	key: string;
	name: string;
	/** Seed for the project's mascot and color: the same root path the desktop hashes. */
	rootPath: string | null;
	icon: string | null;
	color: string | null;
	/** Null for a project the catalog doesn't know (only seen through a workspace). */
	repositoryId: string | null;
	combs: Comb[];
};

/**
 * Projects for the Workspaces tab: every repository the desktop knows, each
 * with its live workspaces, plus any workspace whose project is not in the
 * catalog. Projects with work float up, then by name. A search keeps a
 * project when its name or any of its workspaces match.
 */
export function projectGroups(
	repositories: CatalogRepository[],
	combs: Comb[],
	sessions: SessionSearchResult[],
	query = "",
): ProjectGroup[] {
	const groups = new Map<string, ProjectGroup>();
	const byRoot = new Map<string, string>();
	const byProject = new Map<string, string>();
	for (const repo of repositories) {
		const key = `repo:${repo.id}`;
		groups.set(key, {
			key,
			name: repositoryLabel(repo),
			rootPath: repo.rootPath,
			icon: repo.icon === "logo" && repo.logo ? repo.logo : (repo.icon ?? null),
			color: repo.color ?? null,
			repositoryId: repo.id,
			combs: [],
		});
		byRoot.set(repo.rootPath, key);
		byProject.set(repo.projectId, key);
	}
	for (const comb of combs) {
		if (comb.status === "archived") continue;
		let key =
			(comb.projectPath ? byRoot.get(comb.projectPath) : undefined) ??
			(comb.projectId ? byProject.get(comb.projectId) : undefined);
		if (!key) {
			key = `orphan:${comb.projectPath ?? comb.projectName ?? comb.projectId ?? "—"}`;
			if (!groups.has(key))
				groups.set(key, {
					key,
					name: comb.projectName ?? comb.projectId ?? "—",
					rootPath: comb.projectPath ?? null,
					icon: comb.projectIcon ?? null,
					color: comb.projectColor ?? null,
					repositoryId: null,
					combs: [],
				});
		}
		groups.get(key)!.combs.push(comb);
	}
	const sessionsPer = new Map<string, number>();
	for (const s of sessions)
		if (s.workspaceId) sessionsPer.set(s.workspaceId, (sessionsPer.get(s.workspaceId) ?? 0) + 1);
	const q = query.trim().toLowerCase();
	const matches = (text: Array<string | null | undefined>) =>
		text.filter(Boolean).join(" ").toLowerCase().includes(q);
	const result: ProjectGroup[] = [];
	for (const group of groups.values()) {
		const combsShown = q
			? matches([group.name])
				? group.combs
				: group.combs.filter((c) => matches([c.name, c.branch]))
			: group.combs;
		if (q && combsShown.length === 0 && !matches([group.name])) continue;
		const sorted = [...combsShown].sort((a, b) => {
			const an = sessionsPer.get(a.id) ?? 0;
			const bn = sessionsPer.get(b.id) ?? 0;
			if (an > 0 !== bn > 0) return bn - an;
			return (b.lastOpenedAt ?? "").localeCompare(a.lastOpenedAt ?? "");
		});
		result.push({ ...group, combs: sorted });
	}
	return result.sort((a, b) => {
		if (a.combs.length > 0 !== b.combs.length > 0) return a.combs.length > 0 ? -1 : 1;
		return a.name.localeCompare(b.name);
	});
}
