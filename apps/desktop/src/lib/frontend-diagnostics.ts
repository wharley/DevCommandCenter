export const FRONTEND_DIAGNOSTICS_KEY = "dcc.frontend-errors.v1";
const MAX_RECORDS = 10;

export type FrontendErrorContext = {
	appVersion?: string | null;
	workspaceId?: string | null;
	sessionId?: string | null;
	turnId?: string | null;
	turnState?: string | null;
	hydratedHistoryEventCount?: number;
	liveEventCount?: number;
	transport?: "reconciled" | "legacy";
};

export type FrontendErrorRecord = {
	id: string;
	timestamp: string;
	kind: "react-caught" | "react-uncaught" | "react-recoverable" | "window-error" | "unhandled-rejection";
	scope: string;
	message: string;
	stack: string;
	componentStack: string;
	userAgent: string;
	buildSource: string;
	context: FrontendErrorContext;
};

let context: FrontendErrorContext = {};
let memoryRecords: FrontendErrorRecord[] = [];

function scrub(value: string, limit: number): string {
	return value
		.replace(/(\b[a-z][a-z\d+.-]*:\/\/)[^\s/@]+:[^\s/@]+@/gi, "$1[redacted]@")
		.replace(/\b(Bearer\s+)[\w.\-+/=]+/gi, "$1[redacted]")
		.replace(/([?&](?:token|access_token|api_key|key|password|secret)=)[^\s&#]+/gi, "$1[redacted]")
		.slice(0, limit);
}

export function setFrontendErrorContext(next: FrontendErrorContext) {
	context = { ...next };
}

export function readFrontendErrors(): FrontendErrorRecord[] {
	try {
		const stored: unknown = JSON.parse(localStorage.getItem(FRONTEND_DIAGNOSTICS_KEY) ?? "[]");
		if (Array.isArray(stored)) {
			const valid = stored.filter((entry): entry is FrontendErrorRecord =>
				entry !== null && typeof entry === "object" &&
				typeof entry.id === "string" && typeof entry.timestamp === "string" &&
				typeof entry.message === "string" && typeof entry.stack === "string" &&
				typeof entry.componentStack === "string",
			);
			const merged = new Map(valid.map((entry) => [entry.id, entry]));
			for (const entry of memoryRecords) merged.set(entry.id, entry);
			return [...merged.values()].slice(-MAX_RECORDS);
		}
	} catch {
		// Diagnostics must remain available in memory if storage is full or disabled.
	}
	return [...memoryRecords];
}

/** Never serializes event payloads, prompts, tool output, or arbitrary rejection objects. */
export function recordFrontendError(
	kind: FrontendErrorRecord["kind"],
	error: unknown,
	options: { scope?: string; componentStack?: string | null; context?: FrontendErrorContext } = {},
): void {
	try {
		let message = "Unknown error (non-string rejection)";
		let stack = "";
		if (error instanceof Error) {
			message = `${error.name}: ${error.message}`;
			stack = error.stack ?? "";
		} else if (typeof error === "string") {
			message = error;
		}
		const record: FrontendErrorRecord = {
			id: crypto.randomUUID(),
			timestamp: new Date().toISOString(),
			kind,
			scope: scrub(options.scope ?? "app", 500),
			message: scrub(message, 2000),
			stack: scrub(stack, 6000),
			componentStack: scrub(options.componentStack ?? "", 6000),
			userAgent: navigator.userAgent.slice(0, 500),
			buildSource: scrub(document.querySelector<HTMLScriptElement>('script[type="module"][src]')?.src ?? "", 500),
			context: { ...context, ...options.context },
		};
		memoryRecords = [...readFrontendErrors(), record].slice(-MAX_RECORDS);
		try {
			localStorage.setItem(FRONTEND_DIAGNOSTICS_KEY, JSON.stringify(memoryRecords));
		} catch {
			// Keep the in-memory copy for the recovery screen; never throw while reporting.
		}
	} catch {
		// Even an unusual Error getter or an unavailable browser API cannot break recovery.
	}
}

export function exportFrontendErrors(): string {
	return JSON.stringify({ schemaVersion: 1, errors: readFrontendErrors() }, null, 2);
}

export function installFrontendErrorListeners(): () => void {
	const onError = (event: ErrorEvent) => {
		recordFrontendError("window-error", event.error ?? event.message, {
			scope: event.filename ? `${event.filename}:${event.lineno}:${event.colno}` : "window",
		});
	};
	const onRejection = (event: PromiseRejectionEvent) => {
		recordFrontendError("unhandled-rejection", event.reason);
	};
	window.addEventListener("error", onError);
	window.addEventListener("unhandledrejection", onRejection);
	return () => {
		window.removeEventListener("error", onError);
		window.removeEventListener("unhandledrejection", onRejection);
	};
}
