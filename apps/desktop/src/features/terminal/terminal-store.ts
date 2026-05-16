import {
	getDefaultShell,
	getOrCreateTerminalByOwner,
	getTerminalBackendScope,
	killTerminal,
	listenTerminalExit,
	listenTerminalOutput,
	resizeTerminal,
	writeTerminalStdin,
} from "@/lib/terminal-api";

export type TerminalStatus = "idle" | "starting" | "running" | "exited" | "error";

export type TerminalContext = {
	workspaceName: string;
	workspaceBranch: string;
	providerLabel: string | null;
	sessionState: string;
	sessionId: string | null;
};

export type TerminalSnapshot = {
	workspaceId: string;
	workspacePath: string | null;
	ptyId: string | null;
	shell: string | null;
	status: TerminalStatus;
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
};

const entries = new Map<string, TerminalEntry>();
const ptyToWorkspace = new Map<string, string>();
let bridgePromise: Promise<void> | null = null;
let bridgeScope: string | null = null;
let bridgeCleanup: (() => void) | null = null;

function workspaceEntryKey(workspaceId: string) {
	return `${getTerminalBackendScope()}:${workspaceId}`;
}

function getOrCreateEntry(workspaceId: string): TerminalEntry {
	const entryKey = workspaceEntryKey(workspaceId);
	const existing = entries.get(entryKey);
	if (existing) {
		return existing;
	}

	const created: TerminalEntry = {
		workspaceId,
		workspacePath: null,
		ptyId: null,
		shell: null,
		status: "idle",
		exitCode: null,
		chunks: [],
		bufferedBytes: 0,
		truncated: false,
		listeners: new Set(),
		spawnPromise: null,
	};
	entries.set(entryKey, created);
	return created;
}

function snapshot(entry: TerminalEntry): TerminalSnapshot {
	return {
		workspaceId: entry.workspaceId,
		workspacePath: entry.workspacePath,
		ptyId: entry.ptyId,
		shell: entry.shell,
		status: entry.status,
		exitCode: entry.exitCode,
		chunks: [...entry.chunks],
		bufferedBytes: entry.bufferedBytes,
		truncated: entry.truncated,
	};
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

function notifyStatus(entry: TerminalEntry) {
	for (const listener of entry.listeners) {
		listener.onStatusChange(entry.status, entry.exitCode);
		listener.onPtyIdChange?.(entry.ptyId);
	}
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
		const unlistenOutput = await listenTerminalOutput((event) => {
			const entryKey = ptyToWorkspace.get(event.ptyId);
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
			const entryKey = ptyToWorkspace.get(event.ptyId);
			if (!entryKey) {
				return;
			}

			const entry = entries.get(entryKey);
			if (!entry) {
				return;
			}

			ptyToWorkspace.delete(event.ptyId);
			entry.ptyId = null;
			entry.status = "exited";
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

export function attachWorkspaceTerminal(
	workspaceId: string,
	listener: TerminalListener,
): TerminalSnapshot {
	const entry = getOrCreateEntry(workspaceId);
	entry.listeners.add(listener);
	listener.onStatusChange(entry.status, entry.exitCode);
	listener.onPtyIdChange?.(entry.ptyId);
	return snapshot(entry);
}

export function detachWorkspaceTerminal(
	workspaceId: string,
	listener: TerminalListener,
) {
	const entry = entries.get(workspaceEntryKey(workspaceId));
	if (!entry) {
		return;
	}
	entry.listeners.delete(listener);
}

export async function ensureWorkspaceTerminal(
	workspaceId: string,
	workspacePath: string,
	context: TerminalContext,
): Promise<TerminalSnapshot> {
	await ensureTerminalBridge();
	const entry = getOrCreateEntry(workspaceId);
	entry.workspacePath = workspacePath;

	if (entry.ptyId && entry.status === "running") {
		return snapshot(entry);
	}

	if (entry.spawnPromise) {
		return entry.spawnPromise;
	}

	entry.status = "starting";
	entry.exitCode = null;
	notifyStatus(entry);

	entry.spawnPromise = (async () => {
		try {
			const { shell } = await getDefaultShell();
			const shouldUseLoginArgs = /[\\/](zsh|bash|sh)$/i.test(shell);
			const ownerKey = `workspace:${workspaceId}`;
			const result = await getOrCreateTerminalByOwner(ownerKey, {
				cwd: workspacePath,
				command: shell,
				args: shouldUseLoginArgs ? ["-l"] : [],
				cols: 120,
				rows: 32,
				ptyOwnerKey: ownerKey,
			});

			entry.ptyId = result.ptyId;
			entry.shell = shell;
			entry.status = result.session.status === "exited" ? "exited" : "running";
			entry.exitCode = result.session.lastExitCode ?? null;
			ptyToWorkspace.set(result.ptyId, workspaceEntryKey(workspaceId));

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
						`cwd: ${workspacePath}`,
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
			entry.status = "error";
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

export function getWorkspaceTerminalSnapshot(
	workspaceId: string,
): TerminalSnapshot | null {
	const entry = entries.get(workspaceEntryKey(workspaceId));
	return entry ? snapshot(entry) : null;
}

export function clearWorkspaceTerminal(workspaceId: string) {
	const entry = entries.get(workspaceEntryKey(workspaceId));
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
		void writeTerminalStdin(entry.ptyId, "\x1b[2J\x1b[H");
	}
}

export function writeWorkspaceTerminalInput(workspaceId: string, data: string) {
	const entry = entries.get(workspaceEntryKey(workspaceId));
	if (!entry?.ptyId) {
		return;
	}

	void writeTerminalStdin(entry.ptyId, data);
}

export function resizeWorkspaceTerminal(
	workspaceId: string,
	cols: number,
	rows: number,
) {
	const entry = entries.get(workspaceEntryKey(workspaceId));
	if (!entry?.ptyId) {
		return;
	}

	void resizeTerminal(entry.ptyId, cols, rows);
}

export function killWorkspaceTerminal(workspaceId: string) {
	const entry = entries.get(workspaceEntryKey(workspaceId));
	if (!entry?.ptyId) {
		return;
	}

	const ptyId = entry.ptyId;
	entry.ptyId = null;
	entry.status = "exited";
	entry.exitCode = entry.exitCode ?? -1;
	ptyToWorkspace.delete(ptyId);
	notifyStatus(entry);
	void killTerminal(ptyId);
}
