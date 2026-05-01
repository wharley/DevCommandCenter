import { useCallback, useEffect, useRef, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import {
	TerminalOutput,
	type TerminalHandle,
} from "@/components/terminal-output";
import {
	attachWorkspaceTerminal,
	clearWorkspaceTerminal,
	detachWorkspaceTerminal,
	ensureWorkspaceTerminal,
	type TerminalListener,
	type TerminalSnapshot,
	type TerminalStatus,
	writeWorkspaceTerminalInput,
	resizeWorkspaceTerminal,
	TERMINAL_OUTPUT_TRUNCATION,
} from "./terminal-store";

type TerminalPanelProps = {
	workspaceId: string;
	workspaceName: string;
	workspaceBranch: string;
	workspacePath: string | null;
	providerLabel: string | null;
	sessionState: string;
	sessionId: string | null;
};

export function TerminalPanel({
	workspaceId,
	workspaceName,
	workspaceBranch,
	workspacePath,
	providerLabel,
	sessionState,
	sessionId,
}: TerminalPanelProps) {
	const terminalRef = useRef<TerminalHandle | null>(null);
	const pendingWritesRef = useRef<string[]>([]);
	const pendingFlushFrameRef = useRef<number | null>(null);
	const bootstrappingRef = useRef(false);
	const bootstrappingWritesRef = useRef<string[]>([]);
	const [terminalSnapshot, setTerminalSnapshot] =
		useState<TerminalSnapshot | null>(null);

	const flushPendingWrites = useCallback(() => {
		const terminal = terminalRef.current;
		if (!terminal) {
			return false;
		}

		const pending = pendingWritesRef.current.splice(0);
		if (pending.length === 0) {
			return true;
		}

		for (const chunk of pending) {
			terminal.write(chunk);
		}
		return true;
	}, []);

	const scheduleFlush = useCallback(() => {
		if (pendingFlushFrameRef.current !== null) {
			return;
		}

		const tick = () => {
			pendingFlushFrameRef.current = null;
			if (!flushPendingWrites()) {
				pendingFlushFrameRef.current = requestAnimationFrame(tick);
			}
		};

		pendingFlushFrameRef.current = requestAnimationFrame(tick);
	}, [flushPendingWrites]);

	const queueWrite = useCallback(
		(data: string) => {
			pendingWritesRef.current.push(data);
			scheduleFlush();
		},
		[scheduleFlush],
	);

	const replayChunks = useCallback(
		(chunks: string[]) => {
			if (chunks.length === 0) {
				return;
			}

			const terminal = terminalRef.current;
			if (!terminal) {
				pendingWritesRef.current.push(...chunks);
				scheduleFlush();
				return;
			}

			for (const chunk of chunks) {
				terminal.write(chunk);
			}
		},
		[scheduleFlush],
	);

	const listenerRef = useRef<TerminalListener>({
		onChunk: (data: string) => {
			if (bootstrappingRef.current) {
				bootstrappingWritesRef.current.push(data);
				return;
			}

			queueWrite(data);
		},
		onStatusChange: () => {},
		onPtyIdChange: () => {},
	});

	useEffect(() => {
		listenerRef.current = {
			onChunk: (data: string) => {
				if (bootstrappingRef.current) {
					bootstrappingWritesRef.current.push(data);
					return;
				}

				queueWrite(data);
			},
			onStatusChange: (status: TerminalStatus, exitCode: number | null) => {
				setTerminalSnapshot((current) =>
					current
						? {
								...current,
								status,
								exitCode,
							}
						: current,
				);
			},
			onPtyIdChange: (ptyId: string | null) => {
				setTerminalSnapshot((current) =>
					current
						? {
								...current,
								ptyId,
							}
						: current,
				);
			},
		};
	}, [queueWrite]);

	useEffect(() => {
		let disposed = false;

		const bootstrap = async () => {
			bootstrappingRef.current = true;
			bootstrappingWritesRef.current = [];

			if (!workspacePath) {
				setTerminalSnapshot(null);
				pendingWritesRef.current = [];
				pendingFlushFrameRef.current = null;
				terminalRef.current?.clear();
				bootstrappingRef.current = false;
				return;
			}

			const initial = attachWorkspaceTerminal(workspaceId, listenerRef.current);
			setTerminalSnapshot(initial);

			const next = await ensureWorkspaceTerminal(workspaceId, workspacePath, {
				workspaceName,
				workspaceBranch,
				providerLabel,
				sessionState,
				sessionId,
			});

			if (disposed) {
				bootstrappingRef.current = false;
				return;
			}

			setTerminalSnapshot(next);
			const replay = [
				...(initial.truncated ? [TERMINAL_OUTPUT_TRUNCATION] : []),
				...initial.chunks,
				...bootstrappingWritesRef.current.splice(0),
			];
			pendingWritesRef.current = [];
			pendingFlushFrameRef.current = null;
			terminalRef.current?.clear();
			replayChunks(replay);
			bootstrappingRef.current = false;
		};

		void bootstrap();

		return () => {
			disposed = true;
			bootstrappingRef.current = false;
			detachWorkspaceTerminal(workspaceId, listenerRef.current);
		};
	}, [
		replayChunks,
		workspaceBranch,
		workspaceId,
		workspaceName,
		workspacePath,
	]);

	const handleFocusTerminal = () => {
		terminalRef.current?.focus();
		flushPendingWrites();
	};

	const handleClearTerminal = () => {
		clearWorkspaceTerminal(workspaceId);
		terminalRef.current?.clear();
		pendingWritesRef.current = [];
	};

	const handleTerminalData = useCallback((data: string) => {
		writeWorkspaceTerminalInput(workspaceId, data);
	}, [workspaceId]);

	const handleTerminalResize = useCallback(
		(cols: number, rows: number) => {
			resizeWorkspaceTerminal(workspaceId, cols, rows);
		},
		[workspaceId],
	);

	return (
		<Card className="dcc-terminal">
			<CardHeader>
				<div className="dcc-card__meta-row">
					<div>
						<CardTitle>Workspace terminal</CardTitle>
						<CardDescription>
							Helmor-style xterm shell remembered per workspace and connected to
							the Tauri PTY runtime.
						</CardDescription>
					</div>
					<div className="dcc-terminal__header-actions">
						<Badge variant={terminalSnapshot?.status === "running" ? "success" : "outline"}>
							{terminalSnapshot?.status ?? "idle"}
						</Badge>
						<Badge variant="outline">
							{sessionId ?? "No session"}
						</Badge>
						<Badge variant="outline">
							{terminalSnapshot?.ptyId ?? "No PTY"}
						</Badge>
						<Button
							type="button"
							variant="secondary"
							size="sm"
							onClick={handleFocusTerminal}
						>
							Focus
						</Button>
						<Button
							type="button"
							variant="secondary"
							size="sm"
							onClick={handleClearTerminal}
						>
							Clear
						</Button>
					</div>
				</div>
			</CardHeader>
			<CardContent className="dcc-terminal__viewport">
				<TerminalOutput
					terminalRef={terminalRef}
					className="dcc-terminal__surface"
					detectLinks
					onData={handleTerminalData}
					onResize={handleTerminalResize}
				/>
				<div className="dcc-terminal__note">
					<span>
						<strong>Remembered workspace PTY.</strong> Closing the panel does
						not kill the shell; reopening reattaches to the same runtime.
					</span>
					<Badge variant="outline">
						{workspacePath ?? "Workspace path pending"}
					</Badge>
				</div>
			</CardContent>
		</Card>
	);
}
