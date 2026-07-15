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
	attachTerminal,
	clearTerminal,
	detachTerminal,
	ensureTerminal,
	type TerminalListener,
	type TerminalSnapshot,
	type TerminalStatus,
	writeTerminalInput,
	resizeTerminalView,
	TERMINAL_OUTPUT_TRUNCATION,
} from "./terminal-store";

type TerminalPanelProps = {
	terminalId: string;
	title: string;
	cwd: string | null;
	workspaceName: string;
	workspaceBranch: string;
	providerLabel: string | null;
	sessionState: string;
	sessionId: string | null;
	/** `drawer`: compact chrome for bottom workbench drawer (t3 ThreadTerminalDrawer style). */
	variant?: "card" | "drawer";
	autoFocus?: boolean;
};

export function TerminalPanel({
	terminalId,
	title,
	cwd,
	workspaceName,
	workspaceBranch,
	providerLabel,
	sessionState,
	sessionId,
	variant = "card",
	autoFocus = false,
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

			if (!cwd) {
				setTerminalSnapshot(null);
				pendingWritesRef.current = [];
				pendingFlushFrameRef.current = null;
				terminalRef.current?.clear();
				bootstrappingRef.current = false;
				return;
			}

			const initial = attachTerminal(terminalId, listenerRef.current);
			setTerminalSnapshot(initial);

			const next = await ensureTerminal(terminalId, cwd, {
				title,
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
				...(next.truncated ? [TERMINAL_OUTPUT_TRUNCATION] : []),
				...next.chunks,
				...bootstrappingWritesRef.current.splice(0),
			];
			pendingWritesRef.current = [];
			pendingFlushFrameRef.current = null;
			terminalRef.current?.clear();
			replayChunks(replay);
			bootstrappingRef.current = false;
			if (autoFocus) {
				requestAnimationFrame(() => terminalRef.current?.focus());
			}
		};

		void bootstrap();

		return () => {
			disposed = true;
			bootstrappingRef.current = false;
			detachTerminal(terminalId, listenerRef.current);
		};
	}, [autoFocus, replayChunks, terminalId, cwd, workspaceBranch, workspaceName]);

	const handleFocusTerminal = () => {
		terminalRef.current?.focus();
		flushPendingWrites();
	};

	const handleClearTerminal = () => {
		clearTerminal(terminalId);
		terminalRef.current?.clear();
		pendingWritesRef.current = [];
	};

	const handleTerminalData = useCallback(
		(data: string) => {
			writeTerminalInput(terminalId, data);
		},
		[terminalId],
	);

	const handleTerminalResize = useCallback(
		(cols: number, rows: number) => {
			resizeTerminalView(terminalId, cols, rows);
		},
		[terminalId],
	);

	if (variant === "drawer") {
		return (
			<div className="dcc-terminal dcc-terminal--drawer flex min-h-0 flex-1 flex-col">
				<div className="dcc-terminal__viewport flex min-h-0 flex-1 flex-col overflow-hidden pt-1">
					<TerminalOutput
						terminalRef={terminalRef}
						className="dcc-terminal__surface h-full min-h-0 w-full flex-1"
						detectLinks
						onData={handleTerminalData}
						onResize={handleTerminalResize}
					/>
				</div>
			</div>
		);
	}

	return (
		<Card className="dcc-terminal">
			<CardHeader>
				<div className="dcc-card__meta-row">
					<div>
						<CardTitle className="text-sm font-medium">{title}</CardTitle>
						<CardDescription>
							PTY per tab; panel hide keeps the same shell (Tauri bridge).
						</CardDescription>
					</div>
					<div className="dcc-terminal__header-actions">
						<Badge variant={terminalSnapshot?.status === "running" ? "success" : "outline"}>
							{terminalSnapshot?.status ?? "idle"}
						</Badge>
						<Badge variant="outline">{sessionId ?? "No session"}</Badge>
						<Badge variant="outline">{terminalSnapshot?.ptyId ?? "No PTY"}</Badge>
						<Button type="button" variant="secondary" size="sm" onClick={handleFocusTerminal}>
							Focus
						</Button>
						<Button type="button" variant="secondary" size="sm" onClick={handleClearTerminal}>
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
						<strong>Remembered terminal PTY.</strong> Closing the panel does not kill the
						shell; reopening reattaches to the same runtime.
					</span>
					<Badge variant="outline">{cwd ?? "Terminal path pending"}</Badge>
				</div>
			</CardContent>
		</Card>
	);
}
