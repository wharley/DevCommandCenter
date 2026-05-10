import { useQueryClient } from "@tanstack/react-query";
import type { ForgeCliProvider } from "@dcc/contracts";
import { X, TerminalSquare } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import {
	type TerminalHandle,
	TerminalOutput,
} from "@/components/terminal-output";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import {
	buildForgeCliDisplayCommand,
	buildForgeCliShellCommand,
	getForgeCliStatus,
	normalizeForgeHost,
} from "@/lib/forge-cli";
import { invalidateForgeCliQueries } from "@/features/settings/forge-cli-queries";
import { WORKSPACE_FORGE_CONTEXT_QUERY_KEY } from "@/features/inspector/use-workspace-forge-context";
import {
	getDefaultShell,
	killTerminal,
	listenTerminalExit,
	listenTerminalOutput,
	resizeTerminal,
	spawnTerminal,
	writeTerminalStdin,
} from "@/lib/terminal-api";

type ForgeConnectDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	provider: ForgeCliProvider;
	host: string;
	onConnected?: (info: {
		provider: ForgeCliProvider;
		host: string;
		login: string;
	}) => void;
};

type LoginProbeResult = {
	login: string | null;
};

const NEW_LOGIN_POLL_TIMEOUT_MS = 8_000;
const NEW_LOGIN_POLL_INTERVAL_MS = 1_000;

const sleep = (ms: number) =>
	new Promise<void>((resolve) => setTimeout(resolve, ms));

function providerLabel(provider: ForgeCliProvider): string {
	return provider === "gitlab" ? "GitLab" : "GitHub";
}

function connectedToastMessage(provider: ForgeCliProvider, login: string): string {
	const label = providerLabel(provider);
	return login ? `${label} conectado como @${login}` : `${label} conectado`;
}

async function detectLoginAfterClose(
	provider: ForgeCliProvider,
	host: string,
	baseline: Set<string>,
): Promise<LoginProbeResult> {
	const startedAt = Date.now();
	let lastSeen: string[] = [];

	while (Date.now() - startedAt < NEW_LOGIN_POLL_TIMEOUT_MS) {
		try {
				const next = await getForgeCliStatus(provider, host, {
					forceRefresh: true,
				});
			lastSeen = next.logins ?? [];
			const newLogin = lastSeen.find((login) => !baseline.has(login));
			if (newLogin) {
				return { login: newLogin };
			}
		} catch {
			// CLI state can lag behind PTY exit briefly.
		}

		if (Date.now() - startedAt >= NEW_LOGIN_POLL_TIMEOUT_MS) {
			break;
		}
		await sleep(NEW_LOGIN_POLL_INTERVAL_MS);
	}

	return { login: lastSeen[0] ?? null };
}

export function ForgeConnectDialog({
	open,
	onOpenChange,
	provider,
	host,
	onConnected,
}: ForgeConnectDialogProps) {
	const queryClient = useQueryClient();
	const termRef = useRef<TerminalHandle | null>(null);
	const ptyIdRef = useRef<string | null>(null);
	const baselineRef = useRef<Set<string>>(new Set());
	const pendingOutputRef = useRef<string[]>([]);
	const closeHandledRef = useRef(false);
	const onOpenChangeRef = useRef(onOpenChange);
	const hostValue = normalizeForgeHost(provider, host);
	const [bootError, setBootError] = useState<string | null>(null);

	const flushPendingOutput = useCallback(() => {
		if (!termRef.current || pendingOutputRef.current.length === 0) {
			return;
		}
		for (const chunk of pendingOutputRef.current) {
			termRef.current.write(chunk);
		}
		pendingOutputRef.current = [];
	}, []);

	const appendOutput = useCallback(
		(data: string) => {
			if (!termRef.current) {
				pendingOutputRef.current.push(data);
				return;
			}
			flushPendingOutput();
			termRef.current.write(data);
		},
		[flushPendingOutput],
	);

	const handleClose = useCallback(async () => {
		if (closeHandledRef.current) {
			return;
		}
		closeHandledRef.current = true;

		const ptyId = ptyIdRef.current;
		ptyIdRef.current = null;
		if (ptyId) {
			try {
				await killTerminal(ptyId);
			} catch {
				// PTY may already have exited.
			}
		}

		onOpenChange(false);

		const probe = await detectLoginAfterClose(provider, hostValue, baselineRef.current);
		await invalidateForgeCliQueries(queryClient, provider, hostValue);
		await queryClient.invalidateQueries({
			queryKey: [WORKSPACE_FORGE_CONTEXT_QUERY_KEY],
		});

		if (probe.login) {
			toast.success(connectedToastMessage(provider, probe.login));
			onConnected?.({ provider, host: hostValue, login: probe.login });
		}
	}, [hostValue, onConnected, onOpenChange, provider, queryClient]);

	useEffect(() => {
		onOpenChangeRef.current = (next) => {
			if (!next) {
				void handleClose();
				return;
			}
			onOpenChange(next);
		};
	}, [handleClose, onOpenChange]);

	useEffect(() => {
		if (!open) {
			return;
		}

		closeHandledRef.current = false;
		baselineRef.current = new Set();
		pendingOutputRef.current = [];
		setBootError(null);
		termRef.current?.clear();

		let cancelled = false;

		void getForgeCliStatus(provider, hostValue)
			.then((status) => {
				if (!cancelled) {
					baselineRef.current = new Set(status.logins ?? []);
				}
			})
			.catch(() => {
				if (!cancelled) {
					baselineRef.current = new Set();
				}
			});

		void (async () => {
			try {
				const shell = await getDefaultShell()
					.then((result) => result.shell)
					.catch(() => "/bin/zsh");
				const result = await spawnTerminal({
					cwd: "/",
					command: shell,
					args: ["-lc", buildForgeCliShellCommand(provider, hostValue)],
					cols: 96,
					rows: 24,
					ptyOwnerKey: `forge-connect:${provider}:${hostValue}`,
				});

				if (cancelled || closeHandledRef.current) {
					void killTerminal(result.ptyId).catch(() => {});
					return;
				}

				ptyIdRef.current = result.ptyId;
				requestAnimationFrame(() => {
					termRef.current?.focus();
					flushPendingOutput();
				});
			} catch (error) {
				if (cancelled) {
					return;
				}
				const message =
					error instanceof Error ? error.message : "Unable to start login terminal.";
				setBootError(message);
				appendOutput(`\r\n${message}\r\n`);
			}
		})();

		return () => {
			cancelled = true;
		};
	}, [appendOutput, flushPendingOutput, hostValue, open, provider]);

	useEffect(() => {
		if (!open) {
			return;
		}

		let disposed = false;
		let stopOutput: (() => void) | null = null;
		let stopExit: (() => void) | null = null;

		void listenTerminalOutput((event) => {
			if (disposed || event.ptyId !== ptyIdRef.current) {
				return;
			}
			appendOutput(event.data);
		}).then((unlisten) => {
			if (disposed) {
				unlisten();
				return;
			}
			stopOutput = unlisten;
		});

		void listenTerminalExit((event) => {
			if (disposed || event.ptyId !== ptyIdRef.current) {
				return;
			}
			ptyIdRef.current = null;
			if (event.code === 0) {
				onOpenChangeRef.current(false);
			}
		}).then((unlisten) => {
			if (disposed) {
				unlisten();
				return;
			}
			stopExit = unlisten;
		});

		return () => {
			disposed = true;
			stopOutput?.();
			stopExit?.();
		};
	}, [appendOutput, open]);

	useEffect(() => {
		if (!open) {
			return;
		}
		const id = window.setInterval(() => {
			flushPendingOutput();
		}, 50);
		return () => window.clearInterval(id);
	}, [flushPendingOutput, open]);

	useEffect(() => {
		return () => {
			const ptyId = ptyIdRef.current;
			if (!ptyId) {
				return;
			}
			void killTerminal(ptyId).catch(() => {});
		};
	}, []);

	const handleOpenChange = useCallback((next: boolean) => {
		onOpenChangeRef.current(next);
	}, []);

	const onTerminalData = useCallback((data: string) => {
		const ptyId = ptyIdRef.current;
		if (!ptyId) {
			return;
		}
		void writeTerminalStdin(ptyId, data);
	}, []);

	const onTerminalResize = useCallback((cols: number, rows: number) => {
		const ptyId = ptyIdRef.current;
		if (!ptyId) {
			return;
		}
		void resizeTerminal(ptyId, cols, rows);
	}, []);

	return (
		<Dialog open={open} onOpenChange={handleOpenChange}>
			<DialogContent
				showCloseButton={false}
				className="w-[720px] max-w-[calc(100vw-3rem)] gap-0 overflow-hidden p-0 sm:max-w-[720px]"
			>
				<DialogTitle className="sr-only">
					Conectar {providerLabel(provider)}
				</DialogTitle>
				<header className="flex h-11 items-center gap-3 border-b border-border/60 px-4">
					<div className="flex min-w-0 items-center gap-2 text-[12px] font-medium text-foreground">
						<TerminalSquare className="size-3.5 shrink-0" />
						<span>Conectar {providerLabel(provider)}</span>
						<span className="truncate text-muted-foreground/80">{hostValue}</span>
					</div>
					<div className="ml-auto flex items-center gap-3">
						<span className="hidden text-[11px] text-muted-foreground sm:inline">
							{buildForgeCliDisplayCommand(provider, hostValue)}
						</span>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={() => handleOpenChange(false)}
							aria-label="Close"
							className="gap-1.5 px-2 text-muted-foreground hover:text-foreground"
						>
							<X className="size-3.5" strokeWidth={1.8} />
						</Button>
					</div>
				</header>
				<div className="bg-card">
					<TerminalOutput
						terminalRef={termRef}
						className="h-[380px]"
						detectLinks
						fontSize={12}
						lineHeight={1.35}
						padding="12px 0 12px 16px"
						onData={onTerminalData}
						onResize={onTerminalResize}
					/>
				</div>
				{bootError ? (
					<div className="border-t border-border/60 px-4 py-3 text-[12px] text-destructive">
						{bootError}
					</div>
				) : null}
			</DialogContent>
		</Dialog>
	);
}
