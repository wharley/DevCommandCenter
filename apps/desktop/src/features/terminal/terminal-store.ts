import {
	getDefaultShell,
	getOrCreateTerminalByOwner,
	getTerminalBackendScope,
	killTerminal as killTerminalApi,
	listenTerminalExit,
	listenTerminalOutput,
	resizeTerminal as resizeTerminalApi,
	writeTerminalStdin,
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
const ptyToTerminal = new Map<string, string>();
let bridgePromise: Promise<void> | null = null;
let bridgeScope: string | null = null;
let bridgeCleanup: (() => void) | null = null;

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
		terminalId: entry.terminalId,
		cwd: entry.cwd,
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
	entry.exitCode = null;
	notifyStatus(entry);

	entry.spawnPromise = (async () => {
		try {
			const { shell } = await getDefaultShell();
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

			entry.ptyId = result.ptyId;
			entry.shell = shell;
			entry.status = result.session.status === "exited" ? "exited" : "running";
			entry.exitCode = result.session.lastExitCode ?? null;
			ptyToTerminal.set(result.ptyId, terminalEntryKey(terminalId));

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

export function getTerminalSnapshot(terminalId: string): TerminalSnapshot | null {
	const entry = entries.get(terminalEntryKey(terminalId));
	return entry ? snapshot(entry) : null;
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
		void writeTerminalStdin(entry.ptyId, "\x1b[2J\x1b[H");
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
	const entry = entries.get(terminalEntryKey(terminalId));
	if (!entry?.ptyId) {
		return;
	}

	void resizeTerminalApi(entry.ptyId, cols, rows);
}

export function killTerminal(terminalId: string) {
	const entry = entries.get(terminalEntryKey(terminalId));
	if (!entry?.ptyId) {
		return;
	}

	const ptyId = entry.ptyId;
	entry.ptyId = null;
	entry.status = "exited";
	entry.exitCode = entry.exitCode ?? -1;
	ptyToTerminal.delete(ptyId);
	notifyStatus(entry);
	void killTerminalApi(ptyId);
}
