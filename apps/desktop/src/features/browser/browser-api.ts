import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type BrowserBounds = {
	x: number;
	y: number;
	width: number;
	height: number;
};

export type BrowserSnapshot = {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
	visible: boolean;
	loading: boolean;
	url: string | null;
	title: string | null;
};

export type BrowserOpenRequest = {
	requestId: string;
	workspaceId: string;
	sessionId: string;
	providerId: string;
	url: string;
	reason: string;
	expiresAtMs: number;
};

/** Metadata only: no cookie or credential values cross the native boundary. */
export type BrowserSessionImportProfile = {
	id: string;
	browser: string;
	name: string;
};

export type BrowserSessionProfilesResult = {
	profiles: BrowserSessionImportProfile[];
	supported: boolean;
	message?: string | null;
};

export type BrowserSessionImportResult = {
	imported: number;
	skipped: number;
	supported: boolean;
	message?: string | null;
};

/** In-memory, lifecycle-bound Browser control consent; no capability token is exposed. */
export type BrowserControlStatus = {
	armed: boolean;
	remainingMs: number;
};

/** Closed, content-free audit vocabulary produced by the trusted Browser backend. */
export type BrowserAuditOrigin = "ui" | "mcp";
export type BrowserAuditTool =
	| "dcc_browser_context"
	| "dcc_browser_navigate"
	| "dcc_browser_reload"
	| "dcc_browser_scroll"
	| "dcc_browser_click"
	| "dcc_browser_fill"
	| "dcc_browser_select"
	| "dcc_browser_press"
	| "dcc_browser_evidence_start"
	| "dcc_browser_evidence_read"
	| "browser_arm_control"
	| "browser_disarm_control";
export type BrowserAuditGrantState = "armed" | "expired" | "missing" | "notApplicable";
export type BrowserAuditOutcome = "executed" | "rejected" | "stale" | "notArmed" | "failed";

/** No URL, page content, reference, request payload, or bearer/lease data is rendered from audit records. */
export type BrowserAuditRecord = {
	origin: BrowserAuditOrigin;
	providerId: string | null;
	tool: BrowserAuditTool;
	grantState: BrowserAuditGrantState;
	outcome: BrowserAuditOutcome;
	timestampMs: number;
};

/** Bounded page context obtained by the backend after an explicit user action. */
export type BrowserAgentContext = {
	workspaceId: string;
	sessionId: string | null;
	url: string;
	title: string | null;
	text: string;
	selectionOnly: boolean;
	truncated: boolean;
	semanticMap: BrowserSemanticMap;
};

/** Opaque, page-local references; these are not DOM selectors or automation locators. */
export type BrowserSemanticMap = {
	mapId: string;
	generation: number;
	/** Backend page-load revision; together with mapId/generation it is an action anchor. */
	pageLoadRevision: number;
	items: BrowserSemanticItem[];
	truncated: boolean;
};

export type BrowserSemanticItem = {
	reference: string;
	role: string;
	level?: number;
	name: string;
	destination?: string;
	disabled?: boolean;
	checked?: boolean;
	selected?: boolean;
	expanded?: boolean;
	pressed?: boolean;
	/** Visible select choices supplied by the bounded native semantic map. */
	options?: string[];
};

export type BrowserControlAction =
	| { kind: "navigate"; url: string }
	| { kind: "reload" }
	| { kind: "scroll"; deltaX: number; deltaY: number }
	| { kind: "click"; reference: string }
	| { kind: "fill"; reference: string; text: string }
	| { kind: "select"; reference: string; label: string }
	| { kind: "press"; reference: string; key: "Enter" | "Tab" | "Shift+Tab" | "Escape" | "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" | "Backspace" | "Delete" | "Home" | "End" | "Space" };

/**
 * Complete identity of a fresh semantic map. It is consumed by the backend
 * before any effect, so a capture can never reuse a stale page anchor.
 */
export type BrowserActionAnchor = {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
	mapId: string;
	generation: number;
	url: string;
	pageLoadRevision: number;
};

/** One-shot, short-lived capture handle; the page token never leaves the backend. */
export type BrowserEvidenceCaptureHandle = {
	captureId: string;
	remainingMs: number;
};

/** Bounded, redacted, untrusted page events drained exactly once. */
export type BrowserEvidenceEvent = {
	kind: string;
	sequence: number;
	message: string;
	url?: string;
	line?: number;
	column?: number;
	initiatorType?: string;
	durationMs?: number;
	status?: number;
};

export type BrowserEvidenceResult = {
	events: BrowserEvidenceEvent[];
	truncated: boolean;
	untrusted: boolean;
};

export function anchorFromBrowserContext(
	context: BrowserAgentContext,
	lifecycleToken: number,
): BrowserActionAnchor {
	return {
		workspaceId: context.workspaceId,
		sessionId: context.sessionId,
		lifecycleToken,
		mapId: context.semanticMap.mapId,
		generation: context.semanticMap.generation,
		url: context.url,
		pageLoadRevision: context.semanticMap.pageLoadRevision,
	};
}

/** Starts a console/resource evidence capture; requires armed control and a fresh anchor. */
export function startBrowserEvidenceCapture(anchor: BrowserActionAnchor) {
	return invoke<BrowserEvidenceCaptureHandle>("browser_start_evidence_capture", {
		anchor,
	});
}

/** Drains one capture; a second read, expiry, close or navigation fails closed. */
export function readBrowserEvidenceCapture(input: {
	workspaceId: string;
	sessionId: string | null;
	captureId: string;
}) {
	return invoke<BrowserEvidenceResult>("browser_read_evidence_capture", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		captureId: input.captureId,
	});
}

export function openBrowser(input: {
	workspaceId: string;
	sessionId: string | null;
	initialUrl?: string | null;
	restoreLastUrl?: boolean;
	bounds: BrowserBounds;
	initialOccluded?: boolean;
}) {
	return invoke<BrowserSnapshot>("browser_open", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		initialUrl: input.initialUrl ?? null,
		restoreLastUrl: input.restoreLastUrl ?? false,
		bounds: input.bounds,
		...(input.initialOccluded ? { initialOccluded: true } : {}),
	});
}

export function listBrowserOpenRequests(input: { workspaceId: string; sessionId: string }) {
	return invoke<BrowserOpenRequest[]>("browser_agent_pending", { input });
}

export function listAllBrowserOpenRequests() {
	return invoke<BrowserOpenRequest[]>("browser_agent_pending_all");
}

export type BrowserSessionImportScope = {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
};

export function listBrowserSessionImportProfiles(input: BrowserSessionImportScope) {
	return invoke<BrowserSessionProfilesResult>("browser_session_profiles", input);
}

export function importBrowserSession(input: BrowserSessionImportScope & {
	profileId: string;
	currentUrl: string;
}) {
	return invoke<BrowserSessionImportResult>("browser_session_import", input);
}

export function resolveBrowserOpenRequest(input: {
	requestId: string; workspaceId: string; sessionId: string;
	decision: "allow" | "deny"; lifecycleToken?: number;
}) {
	return invoke<{ status: string; lifecycleToken?: number; remainingMs: number; message?: string }>("browser_agent_resolve", { input });
}

export function acknowledgeBrowserOpen(input: {
	requestId: string;
	workspaceId: string;
	sessionId: string;
	lifecycleToken: number;
}) {
	return resolveBrowserOpenRequest({ ...input, decision: "allow" }).then((result) => {
		if (result.status !== "approved" || result.lifecycleToken !== input.lifecycleToken) {
			throw new Error(result.message || "Browser approval could not be confirmed");
		}
		return result;
	});
}

export function navigateBrowser(input: {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
	url: string;
}) {
	return invoke<BrowserSnapshot>("browser_navigate", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		lifecycleToken: input.lifecycleToken,
		url: input.url,
	});
}

export function reloadBrowser(input: {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
}) {
	return invoke<BrowserSnapshot>("browser_reload", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		lifecycleToken: input.lifecycleToken,
	});
}

export function getBrowserControlStatus(input: {
	workspaceId: string;
	sessionId: string;
	lifecycleToken: number;
}) {
	return invoke<BrowserControlStatus>("browser_control_status", input);
}

export function armBrowserControl(input: {
	workspaceId: string;
	sessionId: string;
	lifecycleToken: number;
}) {
	return invoke<BrowserControlStatus>("browser_arm_control", input);
}

export function disarmBrowserControl(input: {
	workspaceId: string;
	sessionId: string;
	lifecycleToken: number;
}) {
	return invoke<BrowserControlStatus>("browser_disarm_control", input);
}

/** Reads a bounded, newest-first audit snapshot on explicit viewer open/refresh only. */
export function readBrowserAudit(input: {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
	limit: 50;
}) {
	return invoke<BrowserAuditRecord[]>("browser_read_audit", input);
}

export function extractBrowserContext(input: {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
}) {
	return invoke<BrowserAgentContext>("browser_extract_context", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		lifecycleToken: input.lifecycleToken,
	});
}

export function setBrowserBounds(input: {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
	bounds: BrowserBounds;
}) {
	return invoke<void>("browser_set_bounds", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		lifecycleToken: input.lifecycleToken,
		bounds: input.bounds,
	});
}

export function hideBrowser(input: {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
}) {
	return invoke<BrowserSnapshot>("browser_hide", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		lifecycleToken: input.lifecycleToken,
	});
}

/** Temporarily occludes the native child view while a marked DCC portal overlaps it. */
export function setBrowserOccluded(input: {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
	occluded: boolean;
	bounds?: BrowserBounds;
}) {
	return invoke<BrowserSnapshot>("browser_set_occluded", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		lifecycleToken: input.lifecycleToken,
		occluded: input.occluded,
		...(input.bounds ? { bounds: input.bounds } : {}),
	});
}

export function listenBrowserState(
	onState: (snapshot: BrowserSnapshot) => void,
): Promise<UnlistenFn> {
	return listen<BrowserSnapshot>("browser://state-changed", (event) => {
		onState(event.payload);
	});
}
