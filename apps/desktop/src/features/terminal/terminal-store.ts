import {
	getDefaultShell,
	getOrCreateTerminalByOwner,
	getTerminalBackendScope,
	killTerminal as killTerminalApi,
	listTerminalRuntimeActivity,
	listenTerminalExit,
	listenTerminalOutput,
	resizeTerminal as resizeTerminalApi,
	writeTerminalStdin,
	type TerminalRuntimeActivityStatus,
} from "@/lib/terminal-api";

export type TerminalStatus = "idle" | "starting" | "running" | "exited" | "error";

export type TerminalContext = {
	title: string;
	workspaceName: string;
	workspaceBranch: string;
	providerLabel: string | null;
	sessionState: string;
	sessionId: string | null;
};

export type TerminalSnapshot = {
	terminalId: string;
	cwd: string | null;
	ptyId: string | null;
	shell: string | null;
	status: TerminalStatus;
	activityStatus: TerminalRuntimeActivityStatus;
	activityLabel: string | null;
	activityProcessId: number | null;
	exitCode: number | null;
	chunks: string[];
	bufferedBytes: number;
	truncated: boolean;
};

export type TerminalListener = {
	onChunk: (data: string) => void;
	onStatusChange: (status: TerminalStatus, exitCode: number | null) => void;
	onPtyIdChange?: (ptyId: string | null) => void;
};

const MAX_CHUNK_BYTES = 2 * 1024 * 1024;
export const TERMINAL_OUTPUT_TRUNCATION =
	"\r\n\x1b[2m… earlier output truncated (buffer limit reached) …\x1b[0m\r\n";

type TerminalEntry = TerminalSnapshot & {
	listeners: Set<TerminalListener>;
	spawnPromise: Promise<TerminalSnapshot> | null;
	disposed: boolean;
	lastResizeCols: number | null;
	lastResizeRows: number | null;
};

const entries = new Map<string, TerminalEntry>();
const ptyToTerminal = new Map<string, string>();
const storeListeners = new Set<() => void>();
let bridgePromise: Promise<void> | null = null;
let bridgeScope: string | null = null;
let bridgeCleanup: (() => void) | null = null;
let activityPollTimer: ReturnType<typeof setInterval> | null = null;
let activityPollInFlight = false;

function terminalEntryKey(terminalId: string) {
	return `${getTerminalBackendScope()}:${terminalId}`;
}

function getOrCreateEntry(terminalId: string): TerminalEntry {
	const entryKey = terminalEntryKey(terminalId);
	const existing = entries.get(entryKey);
	if (existing) {
		return existing;
	}

	const created: TerminalEntry = {
		terminalId,
		cwd: null,
		ptyId: null,
		shell: null,
		status: "idle",
		activityStatus: "idle",
		activityLabel: null,
		activityProcessId: null,
		exitCode: null,
		chunks: [],
		bufferedBytes: 0,
		truncated: false,
		listeners: new Set(),
		spawnPromise: null,
		disposed: false,
		lastResizeCols: null,
		lastResizeRows: null,
	};
	entries.set(entryKey, created);
	return created;
}

function snapshot(entry: TerminalEntry): TerminalSnapshot {
	return {
		terminalId: entry.terminalId,
		cwd: entry.cwd,
		ptyId: entry.ptyId,
		shell: entry.shell,
		status: entry.status,
		activityStatus: entry.activityStatus,
		activityLabel: entry.activityLabel,
		activityProcessId: entry.activityProcessId,
		exitCode: entry.exitCode,
		chunks: [...entry.chunks],
		bufferedBytes: entry.bufferedBytes,
		truncated: entry.truncated,
	};
}

/**
 * The xterm view can finish its first layout before the asynchronous PTY spawn
 * completes. Keep that layout and apply it as soon as a PTY is available so
 * full-screen terminal apps (for example nano) receive the visible dimensions.
 */
function applyPendingResize(entry: TerminalEntry) {
	if (
		!entry.ptyId ||
		entry.lastResizeCols === null ||
		entry.lastResizeRows === null
	) {
		return;
	}

	void resizeTerminalApi(entry.ptyId, entry.lastResizeCols, entry.lastResizeRows);
}

function appendChunk(entry: TerminalEntry, data: string) {
	entry.chunks.push(data);
	entry.bufferedBytes += data.length;
	while (entry.bufferedBytes > MAX_CHUNK_BYTES && entry.chunks.length > 1) {
		const dropped = entry.chunks.shift();
		if (dropped === undefined) {
			break;
		}
		entry.bufferedBytes -= dropped.length;
		entry.truncated = true;
	}

	for (const listener of entry.listeners) {
		listener.onChunk(data);
	}
}

function notifyStoreListeners() {
	for (const listener of storeListeners) {
		listener();
	}
}

function notifyStatus(entry: TerminalEntry) {
	for (const listener of entry.listeners) {
		listener.onStatusChange(entry.status, entry.exitCode);
		listener.onPtyIdChange?.(entry.ptyId);
	}
	notifyStoreListeners();
}

async function refreshTerminalActivity() {
	if (activityPollInFlight) return;
	activityPollInFlight = true;
	try {
		const activity = await listTerminalRuntimeActivity();
		const byPtyId = new Map(activity.map((item) => [item.ptyId, item]));
		for (const entry of entries.values()) {
			const item = entry.ptyId ? byPtyId.get(entry.ptyId) : null;
			const nextStatus =
				item?.status ?? (entry.status === "exited" ? "exited" : "idle");
			const nextLabel = item?.processLabel ?? null;
			const nextProcessId = item?.processId ?? null;
			if (
				entry.activityStatus === nextStatus &&
				entry.activityLabel === nextLabel &&
				entry.activityProcessId === nextProcessId
			) {
				continue;
			}
			entry.activityStatus = nextStatus;
			entry.activityLabel = nextLabel;
			entry.activityProcessId = nextProcessId;
			notifyStatus(entry);
		}
	} catch {
		// Activity is progressive enhancement; terminal input/output remains usable.
	} finally {
		activityPollInFlight = false;
	}
}

function ensureActivityPolling() {
	if (activityPollTimer !== null) return;
	void refreshTerminalActivity();
	activityPollTimer = setInterval(() => {
		if ([...entries.values()].some((entry) => entry.ptyId !== null)) {
			void refreshTerminalActivity();
		}
	}, 2_000);
}

export function subscribeTerminalStore(listener: () => void): () => void {
	storeListeners.add(listener);
	return () => {
		storeListeners.delete(listener);
	};
}

async function ensureTerminalBridge() {
	const scope = getTerminalBackendScope();
	if (bridgeScope === scope && bridgePromise) {
		return bridgePromise;
	}

	bridgeCleanup?.();
	bridgeCleanup = null;
	bridgeScope = scope;

	bridgePromise = (async () => {
		ensureActivityPolling();
		const unlistenOutput = await listenTerminalOutput((event) => {
			const entryKey = ptyToTerminal.get(event.ptyId);
			if (!entryKey) {
				return;
			}

			const entry = entries.get(entryKey);
			if (!entry) {
				return;
			}

			appendChunk(entry, event.data);
		});

		const unlistenExit = await listenTerminalExit((event) => {
			const entryKey = ptyToTerminal.get(event.ptyId);
			if (!entryKey) {
				return;
			}

			const entry = entries.get(entryKey);
			if (!entry) {
				return;
			}

			ptyToTerminal.delete(event.ptyId);
			entry.ptyId = null;
			entry.status = "exited";
			entry.activityStatus = "exited";
			entry.activityLabel = null;
			entry.activityProcessId = null;
			entry.exitCode = event.code;
			appendChunk(
				entry,
				`\r\n\x1b[2m[Terminal exited with code ${event.code ?? "?"}]\x1b[0m\r\n`,
			);
			notifyStatus(entry);
		});

		bridgeCleanup = () => {
			unlistenOutput();
			unlistenExit();
		};
	})();

	return bridgePromise;
}

export function attachTerminal(
	terminalId: string,
	listener: TerminalListener,
): TerminalSnapshot {
	const entry = getOrCreateEntry(terminalId);
	entry.listeners.add(listener);
	listener.onStatusChange(entry.status, entry.exitCode);
	listener.onPtyIdChange?.(entry.ptyId);
	return snapshot(entry);
}

export function detachTerminal(terminalId: string, listener: TerminalListener) {
	const entry = entries.get(terminalEntryKey(terminalId));
	if (!entry) {
		return;
	}
	entry.listeners.delete(listener);
}

export async function ensureTerminal(
	terminalId: string,
	cwd: string,
	context: TerminalContext,
): Promise<TerminalSnapshot> {
	await ensureTerminalBridge();
	const entry = getOrCreateEntry(terminalId);
	entry.cwd = cwd;

	if (entry.ptyId && entry.status === "running") {
		return snapshot(entry);
	}

	if (entry.spawnPromise) {
		return entry.spawnPromise;
	}

	entry.status = "starting";
	entry.activityStatus = "idle";
	entry.activityLabel = null;
	entry.activityProcessId = null;
	entry.exitCode = null;
	notifyStatus(entry);

	entry.spawnPromise = (async () => {
		try {
			const { shell } = await getDefaultShell();
			if (entry.disposed) {
				return snapshot(entry);
			}
			const shouldUseLoginArgs = /[\\/](zsh|bash|sh)$/i.test(shell);
			const ownerKey = `terminal:${terminalId}`;
			const result = await getOrCreateTerminalByOwner(ownerKey, {
				cwd,
				command: shell,
				args: shouldUseLoginArgs ? ["-l"] : [],
				cols: 120,
				rows: 32,
				ptyOwnerKey: ownerKey,
			});
			if (entry.disposed) {
				await killTerminalApi(result.ptyId).catch(() => ({ ok: false }));
				entry.status = "exited";
				entry.activityStatus = "exited";
				entry.ptyId = null;
				return snapshot(entry);
			}

			entry.ptyId = result.ptyId;
			entry.shell = shell;
			entry.status = result.session.status === "exited" ? "exited" : "running";
			entry.activityStatus = entry.status === "exited" ? "exited" : "idle";
			entry.exitCode = result.session.lastExitCode ?? null;
			ptyToTerminal.set(result.ptyId, terminalEntryKey(terminalId));
			applyPendingResize(entry);

			if (result.chunks.length > 0) {
				entry.chunks = [...result.chunks];
				entry.bufferedBytes = result.chunks.reduce(
					(total, chunk) => total + chunk.length,
					0,
				);
				entry.truncated = result.truncated;
			} else if (!result.existing) {
				appendChunk(
					entry,
					[
						"\x1b[2mDev Command Center terminal\x1b[0m",
						"",
						`workspace: ${context.workspaceName}`,
						`branch: ${context.workspaceBranch}`,
						`cwd: ${cwd}`,
						`provider: ${context.providerLabel ?? "none"}`,
						`session: ${context.sessionState}`,
						`shell: ${shell}`,
						"",
					].join("\r\n") + "\r\n",
				);
			}
			notifyStatus(entry);
			return snapshot(entry);
		} catch (error) {
			if (entry.disposed) {
				return snapshot(entry);
			}
			entry.status = "error";
			entry.activityStatus = "error";
			entry.exitCode = 1;
			appendChunk(
				entry,
				`\r\n\x1b[31mFailed to open terminal: ${String(error)}\x1b[0m\r\n`,
			);
			notifyStatus(entry);
			return snapshot(entry);
		} finally {
			entry.spawnPromise = null;
		}
	})();

	return entry.spawnPromise;
}

export function getTerminalSnapshot(terminalId: string): TerminalSnapshot | null {
	const entry = entries.get(terminalEntryKey(terminalId));
	return entry ? snapshot(entry) : null;
}

export function getAllTerminalSnapshots(): TerminalSnapshot[] {
	return [...entries.values()].map(snapshot);
}

export async function terminateWorkspaceTerminals(workspaceIds: readonly string[]) {
	const prefixes = workspaceIds.map((id) => `worktree:${id}:`);
	const terminations: Array<Promise<boolean>> = [];
	for (const entry of entries.values()) {
		if (
			(entry.ptyId || entry.spawnPromise) &&
			prefixes.some((prefix) => entry.terminalId.startsWith(prefix))
		) {
			terminations.push(disposeTerminal(entry.terminalId));
		}
	}
	const results = await Promise.all(terminations);
	return results.filter(Boolean).length;
}

export function countWorkspaceActiveTerminals(workspaceIds: readonly string[]) {
	const prefixes = workspaceIds.map((id) => `worktree:${id}:`);
	return [...entries.values()].filter(
		(entry) =>
			prefixes.some((prefix) => entry.terminalId.startsWith(prefix)) &&
			(entry.activityStatus === "running" || entry.activityStatus === "waiting"),
	).length;
}

const ANSI_SEQUENCE = /\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))/g;

export function getTerminalContextExcerpt(
	terminalId: string,
	options: { maxLines?: number; maxChars?: number } = {},
): string {
	const entry = entries.get(terminalEntryKey(terminalId));
	if (!entry) return "";
	const maxLines = options.maxLines ?? 120;
	const maxChars = options.maxChars ?? 16_000;
	const plain = entry.chunks.join("").replace(ANSI_SEQUENCE, "").replace(/\r/g, "");
	const lines = plain.split("\n").slice(-maxLines).join("\n").trim();
	return lines.slice(-maxChars);
}

export function clearTerminal(terminalId: string) {
	const entry = entries.get(terminalEntryKey(terminalId));
	if (!entry) {
		return;
	}

	entry.chunks = [];
	entry.bufferedBytes = 0;
	entry.truncated = false;
	entry.listeners.forEach((listener) => {
		listener.onChunk("\x1b[2J\x1b[H");
	});

	if (entry.ptyId) {
		void writeTerminalStdin(entry.ptyId, "\x0c");
	}
}

export function writeTerminalInput(terminalId: string, data: string) {
	const entry = entries.get(terminalEntryKey(terminalId));
	if (!entry?.ptyId) {
		return;
	}

	void writeTerminalStdin(entry.ptyId, data);
}

export function resizeTerminalView(
	terminalId: string,
	cols: number,
	rows: number,
) {
	const entry = getOrCreateEntry(terminalId);
	if (entry.lastResizeCols === cols && entry.lastResizeRows === rows) {
		return;
	}

	entry.lastResizeCols = cols;
	entry.lastResizeRows = rows;
	applyPendingResize(entry);
}

export function killTerminal(terminalId: string): Promise<boolean> {
	const entry = entries.get(terminalEntryKey(terminalId));
	if (!entry?.ptyId) {
		return Promise.resolve(false);
	}

	const ptyId = entry.ptyId;
	entry.ptyId = null;
	entry.status = "exited";
	entry.activityStatus = "exited";
	entry.activityLabel = null;
	entry.activityProcessId = null;
	entry.exitCode = entry.exitCode ?? -1;
	ptyToTerminal.delete(ptyId);
	notifyStatus(entry);
	return killTerminalApi(ptyId)
		.then((result) => result.ok)
		.catch(() => false);
}

/**
 * Permanently forgets a terminal after its owning tab/workspace is removed.
 *
 * `killTerminal` intentionally retains the entry so an exited tab can display
 * its final output or be restarted. Closing the tab is different: keeping that
 * entry would retain up to MAX_CHUNK_BYTES indefinitely in this module-level
 * store. Remove the routing first so late PTY events cannot recreate output in
 * a terminal the user has already closed.
 */
export function disposeTerminal(terminalId: string): Promise<boolean> {
	const entryKey = terminalEntryKey(terminalId);
	const entry = entries.get(entryKey);
	if (!entry) {
		return Promise.resolve(false);
	}

	const ptyId = entry.ptyId;
	const spawnPromise = entry.spawnPromise;
	entry.disposed = true;
	if (ptyId) {
		ptyToTerminal.delete(ptyId);
	}
	entry.listeners.clear();
	entry.chunks = [];
	entry.bufferedBytes = 0;
	entries.delete(entryKey);
	notifyStoreListeners();

	if (!ptyId) {
		return spawnPromise
			? spawnPromise.then(() => true).catch(() => false)
			: Promise.resolve(true);
	}

	return killTerminalApi(ptyId)
		.then((result) => result.ok)
		.catch(() => false);
}

export async function interruptTerminal(terminalId: string) {
	const entry = entries.get(terminalEntryKey(terminalId));
	if (!entry?.ptyId) return false;
	const result = await writeTerminalStdin(entry.ptyId, "\x03");
	return result.ok;
}

export async function restartTerminal(
	terminalId: string,
	cwd: string,
	context: TerminalContext,
) {
	const entry = getOrCreateEntry(terminalId);
	if (entry.ptyId) {
		const ptyId = entry.ptyId;
		await killTerminalApi(ptyId);
		ptyToTerminal.delete(ptyId);
	}
	entry.ptyId = null;
	entry.status = "idle";
	entry.activityStatus = "idle";
	entry.activityLabel = null;
	entry.activityProcessId = null;
	entry.exitCode = null;
	entry.chunks = [];
	entry.bufferedBytes = 0;
	entry.truncated = false;
	notifyStatus(entry);
	return ensureTerminal(terminalId, cwd, context);
}
