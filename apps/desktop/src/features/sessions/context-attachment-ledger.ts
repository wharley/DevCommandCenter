/**
 * Ephemeral metadata for an explicit context attachment.
 *
 * The ledger deliberately never owns the attachment itself. In particular, it
 * must not retain page text, terminal output, URLs, refs, messages, tokens, or
 * evidence. It protects asynchronous provider handoff work from scope or
 * generation changes; Browser/Terminal entries are observational metadata
 * recorded only after their authoritative guards have already accepted them.
 */

export const MAX_CONTEXT_ATTACHMENTS_PER_SCOPE = 32;
export const MAX_CONTEXT_ATTACHMENT_CHARS = 15_000;
export const MAX_TERMINAL_CONTEXT_ATTACHMENT_CHARS = 16_000;
export const MAX_PROVIDER_HANDOFF_CONTEXT_ATTACHMENT_CHARS = 12_000;
export const MAX_CONTEXT_ATTACHMENT_ID_CHARS = 96;
const MAX_CONTEXT_EPOCH = Number.MAX_SAFE_INTEGER;

export type ContextAttachmentSource =
	| "browser"
	| "terminal"
	| "provider_handoff";

export type ContextAttachmentTrust =
	| "remote_untrusted"
	| "local_terminal"
	| "derived_context";

export type ContextAttachmentScope = {
	workspaceId: string;
	sessionId?: string | null;
	providerId?: string | null;
};

export type ContextAttachmentMeta = {
	source: ContextAttachmentSource;
	workspaceId: string;
	sessionId?: string;
	providerId?: string;
	turnId?: string;
	generation: number;
	chars: number;
	truncated: boolean;
	trust: ContextAttachmentTrust;
	consumed: boolean;
};

/** Opaque process-local handle; it is not sent to the backend or provider. */
export type ContextAttachmentId = {
	readonly __contextAttachmentId: unique symbol;
};

type LedgerEntry = ContextAttachmentMeta & {
	id: ContextAttachmentId;
	scopeKey: string;
};

type OrderedId = {
	scopeKey: string;
	id: ContextAttachmentId;
};

function boundedIdentifier(value: string | null | undefined, required: boolean) {
	if (value == null) return required ? null : undefined;
	const trimmed = value.trim();
	if (
		(trimmed.length === 0 && required) ||
		trimmed.length > MAX_CONTEXT_ATTACHMENT_ID_CHARS ||
		[...trimmed].some((character) => character.charCodeAt(0) < 0x20)
	) {
		return null;
	}
	return trimmed || undefined;
}

function normalizedScope(scope: ContextAttachmentScope) {
	const workspaceId = boundedIdentifier(scope.workspaceId, true);
	const sessionId = boundedIdentifier(scope.sessionId, false);
	const providerId = boundedIdentifier(scope.providerId, false);
	if (!workspaceId || sessionId === null || providerId === null) return null;
	return { workspaceId, sessionId, providerId };
}

function scopeKey(scope: ContextAttachmentScope) {
	const normalized = normalizedScope(scope);
	if (!normalized) return null;
	return [
		normalized.workspaceId,
		normalized.sessionId ?? "",
		normalized.providerId ?? "",
	].join("\u001f");
}

function sourceTrustMatches(
	source: ContextAttachmentSource,
	trust: ContextAttachmentTrust,
) {
	return (
		(source === "browser" && trust === "remote_untrusted") ||
		(source === "terminal" && trust === "local_terminal") ||
		(source === "provider_handoff" && trust === "derived_context")
	);
}

export class ContextAttachmentLedger {
	private readonly entries = new Map<ContextAttachmentId, LedgerEntry>();

	private readonly order: OrderedId[] = [];

	private activeScopeKey: string | null = null;

	private activeGeneration = 0;

	private epoch = 0;

	/** Synchronizes the one active scope and invalidates the previous one. */
	syncScope(scope: ContextAttachmentScope) {
		const key = scopeKey(scope);
		if (!key) return null;
		if (this.activeScopeKey !== key) {
			this.clearEntries();
			this.activeScopeKey = key;
			this.activeGeneration = this.advanceEpoch();
		}
		return this.activeGeneration;
	}

	currentGeneration(scope: ContextAttachmentScope) {
		const key = scopeKey(scope);
		return key && key === this.activeScopeKey ? this.activeGeneration : null;
	}

	invalidate(scope: ContextAttachmentScope) {
		const key = scopeKey(scope);
		if (!key || key !== this.activeScopeKey) return;
		this.clearEntries();
		this.activeScopeKey = null;
		this.activeGeneration = this.advanceEpoch();
	}

	clear() {
		this.clearEntries();
		this.activeScopeKey = null;
		this.activeGeneration = this.advanceEpoch();
	}

	issue(input: {
		source: ContextAttachmentSource;
		workspaceId: string;
		sessionId?: string | null;
		providerId?: string | null;
		turnId?: string | null;
		chars: number;
		truncated: boolean;
		trust: ContextAttachmentTrust;
	}) {
		const normalized = normalizedScope(input);
		const turnId = boundedIdentifier(input.turnId, false);
		if (!normalized || turnId === null || !sourceTrustMatches(input.source, input.trust)) {
			return null;
		}
		const key = scopeKey(input);
		if (!key || key !== this.activeScopeKey) return null;
		const generation = this.currentGeneration(input);
		if (generation === null) return null;
		const maxChars =
			input.source === "terminal"
				? MAX_TERMINAL_CONTEXT_ATTACHMENT_CHARS
				: input.source === "provider_handoff"
					? MAX_PROVIDER_HANDOFF_CONTEXT_ATTACHMENT_CHARS
					: MAX_CONTEXT_ATTACHMENT_CHARS;
		const chars = Number.isFinite(input.chars)
			? Math.max(0, Math.min(maxChars, Math.floor(input.chars)))
			: null;
		if (chars === null) return null;

		const id = Object.freeze({}) as ContextAttachmentId;
		const entry: LedgerEntry = {
			id,
			scopeKey: key,
			source: input.source,
			workspaceId: normalized.workspaceId,
			...(normalized.sessionId ? { sessionId: normalized.sessionId } : {}),
			...(normalized.providerId ? { providerId: normalized.providerId } : {}),
			...(turnId ? { turnId } : {}),
			generation,
			chars,
			truncated: Boolean(input.truncated) || input.chars > maxChars,
			trust: input.trust,
			consumed: false,
		};

		while (this.order.length >= MAX_CONTEXT_ATTACHMENTS_PER_SCOPE) {
			const oldest = this.order.shift();
			if (!oldest) break;
			this.entries.delete(oldest.id);
		}
		this.entries.set(id, entry);
		this.order.push({ scopeKey: key, id });
		return id;
	}

	validateCurrent(id: ContextAttachmentId, scope: ContextAttachmentScope) {
		const key = scopeKey(scope);
		if (!key || key !== this.activeScopeKey) return false;
		const entry = this.entries.get(id);
		return Boolean(
			entry &&
			!entry.consumed &&
			entry.generation === this.currentGeneration(scope),
		);
	}

	/** Marks metadata after the caller accepts it for the composer, not after a turn is persisted. */
	consume(id: ContextAttachmentId, scope: ContextAttachmentScope) {
		const key = scopeKey(scope);
		if (!key || key !== this.activeScopeKey) return false;
		const entry = this.entries.get(id);
		if (!entry || entry.consumed || entry.generation !== this.currentGeneration(scope)) {
			return false;
		}
		entry.consumed = true;
		return true;
	}

	metadata(
		id: ContextAttachmentId,
		scope: ContextAttachmentScope,
	): ContextAttachmentMeta | null {
		const key = scopeKey(scope);
		const entry = key && key === this.activeScopeKey ? this.entries.get(id) : undefined;
		if (!entry || entry.scopeKey !== key) return null;
		const { id: _id, scopeKey: _scopeKey, ...meta } = entry;
		return { ...meta };
	}

	private clearEntries() {
		this.entries.clear();
		this.order.length = 0;
	}

	private advanceEpoch() {
		if (this.epoch >= MAX_CONTEXT_EPOCH) {
			// A numeric epoch can only wrap safely after all records are discarded,
			// so stale handles cannot match a new entry.
			this.clearEntries();
			this.activeScopeKey = null;
			this.epoch = 1;
		} else {
			this.epoch += 1;
		}
		return this.epoch;
	}
}
