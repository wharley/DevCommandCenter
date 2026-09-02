import { describe, expect, it } from "vitest";
import {
	ContextAttachmentLedger,
	MAX_CONTEXT_ATTACHMENTS_PER_SCOPE,
} from "./context-attachment-ledger";

const scope = {
	workspaceId: "workspace-1",
	sessionId: "session-1",
	providerId: "codex",
};

function issue(
	ledger: ContextAttachmentLedger,
	overrides: Partial<{
		workspaceId: string;
		sessionId: string | null;
		providerId: string | null;
		chars: number;
	}> = {},
) {
	return ledger.issue({
		source: "provider_handoff",
		...scope,
		chars: 40,
		truncated: false,
		trust: "derived_context",
		...overrides,
	});
}

describe("ContextAttachmentLedger", () => {
	it("requires a valid scope and keeps only metadata", () => {
		const ledger = new ContextAttachmentLedger();
		ledger.syncScope(scope);
		const id = issue(ledger);

		expect(id).not.toBeNull();
		expect(ledger.metadata(id!, scope)).toEqual({
			source: "provider_handoff",
			workspaceId: "workspace-1",
			sessionId: "session-1",
			providerId: "codex",
			generation: 1,
			chars: 40,
			truncated: false,
			trust: "derived_context",
			consumed: false,
		});
		const metadata = ledger.metadata(id!, scope)!;
		const forbiddenKeys = Object.keys(metadata).filter((key) =>
			/^(text|url|ref|dom|output|message|token|evidence|secret)$/iu.test(key),
		);
		expect(forbiddenKeys).toEqual([]);
	});

	it("validates then consumes exactly once", () => {
		const ledger = new ContextAttachmentLedger();
		ledger.syncScope(scope);
		const id = issue(ledger)!;

		expect(ledger.validateCurrent(id, scope)).toBe(true);
		expect(ledger.consume(id, scope)).toBe(true);
		expect(ledger.validateCurrent(id, scope)).toBe(false);
		expect(ledger.consume(id, scope)).toBe(false);
		expect(ledger.metadata(id, scope)?.consumed).toBe(true);
	});

	it("invalidates stale async work on scope or generation changes", () => {
		const ledger = new ContextAttachmentLedger();
		ledger.syncScope(scope);
		const id = issue(ledger)!;

		ledger.syncScope({ ...scope, sessionId: "session-2" });
		expect(ledger.validateCurrent(id, scope)).toBe(false);
		ledger.syncScope(scope);
		const next = issue(ledger)!;
		ledger.invalidate(scope);
		expect(ledger.validateCurrent(next, scope)).toBe(false);
		expect(ledger.currentGeneration(scope)).toBeNull();
		ledger.syncScope(scope);
		expect(ledger.currentGeneration(scope)).toBe(5);
	});

	it("evicts deterministically at the per-scope cap", () => {
		const ledger = new ContextAttachmentLedger();
		ledger.syncScope(scope);
		const ids = Array.from(
			{ length: MAX_CONTEXT_ATTACHMENTS_PER_SCOPE + 1 },
			() => issue(ledger)!,
		);

		expect(ledger.metadata(ids[0]!, scope)).toBeNull();
		expect(ledger.metadata(ids.at(-1)!, scope)).not.toBeNull();
	});

	it("rejects invalid identifiers and bounds character metadata", () => {
		const ledger = new ContextAttachmentLedger();
		expect(issue(ledger, { workspaceId: "" })).toBeNull();
		ledger.syncScope(scope);
		const id = issue(ledger, { chars: Number.POSITIVE_INFINITY });
		expect(id).toBeNull();
		const bounded = issue(ledger, { chars: 999_999 });
		expect(ledger.metadata(bounded!, scope)?.chars).toBe(12_000);
		expect(ledger.metadata(bounded!, scope)?.truncated).toBe(true);
	});

	it("rejects a semantically mismatched source and trust", () => {
		const ledger = new ContextAttachmentLedger();
		ledger.syncScope(scope);
		expect(
			ledger.issue({
				source: "browser",
				...scope,
				chars: 1,
				truncated: false,
				trust: "local_terminal",
			}),
		).toBeNull();
		expect(
			ledger.issue({
				source: "diff",
				...scope,
				chars: 1,
				truncated: false,
				trust: "remote_untrusted",
			}),
		).toBeNull();
		expect(
			ledger.issue({
				source: "diff",
				...scope,
				chars: 1,
				truncated: false,
				trust: "local_workspace",
			}),
		).not.toBeNull();
	});

	it("clears all ephemeral records without affecting callers", () => {
		const ledger = new ContextAttachmentLedger();
		ledger.syncScope(scope);
		const id = issue(ledger)!;
		const beforeClear = ledger.metadata(id, scope)?.generation;
		ledger.clear();
		expect(ledger.metadata(id, scope)).toBeNull();
		expect(ledger.currentGeneration(scope)).toBeNull();
		ledger.syncScope(scope);
		const next = issue(ledger)!;
		expect(ledger.validateCurrent(id, scope)).toBe(false);
		expect(ledger.metadata(next, scope)?.generation).not.toBe(beforeClear);
	});
});
