import { useQueryClient } from "@tanstack/react-query";
import { Rabbit, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	type TerminalHandle,
	TerminalOutput,
} from "@/components/terminal-output";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { ShortcutDisplay } from "@/features/shortcuts/shortcut-display";
import { invalidateCodeRabbitCliQueries } from "@/features/settings/coderabbit-cli-queries";
import { buildCodeRabbitLoginShellCommand, getCodeRabbitCliStatus } from "@/lib/coderabbit-cli";
import {
	getDefaultShell,
	killTerminal,
	listenTerminalExit,
	listenTerminalOutput,
	resizeTerminal,
	spawnTerminal,
	writeTerminalStdin,
} from "@/lib/terminal-api";

type CodeRabbitConnectDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	workspaceRoot?: string | null;
	onConnected?: () => void;
};

const LOGIN_POLL_TIMEOUT_MS = 8_000;
const LOGIN_POLL_INTERVAL_MS = 1_000;

const sleep = (ms: number) =>
	new Promise<void>((resolve) => setTimeout(resolve, ms));

async function detectCodeRabbitAuth(workspaceRoot?: string | null): Promise<boolean> {
	const startedAt = Date.now();

	while (Date.now() - startedAt < LOGIN_POLL_TIMEOUT_MS) {
		try {
			const status = await getCodeRabbitCliStatus({
				workspaceRoot,
				includeAuthStatus: true,
			});
			if (status.auth?.success || status.auth?.authenticated) {
				return true;
			}
		} catch {
			// CLI auth state can lag behind PTY exit briefly.
		}
		await sleep(LOGIN_POLL_INTERVAL_MS);
	}

	return false;
}

export function CodeRabbitConnectDialog({
	open,
	onOpenChange,
	workspaceRoot,
	onConnected,
}: CodeRabbitConnectDialogProps) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const termRef = useRef<TerminalHandle | null>(null);
	const ptyIdRef = useRef<string | null>(null);
	const pendingOutputRef = useRef<string[]>([]);
	const closeHandledRef = useRef(false);
	const onOpenChangeRef = useRef(onOpenChange);
	const root = workspaceRoot?.trim() || null;
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

		const connected = await detectCodeRabbitAuth(root);
		await invalidateCodeRabbitCliQueries(queryClient, root);

		if (connected) {
			toast.success(t("settings.codeRabbit.connectedToast"));
			onConnected?.();
		}
	}, [onConnected, onOpenChange, queryClient, root, t]);

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
		pendingOutputRef.current = [];
		setBootError(null);
		termRef.current?.clear();

		let cancelled = false;

		void (async () => {
			try {
				const shell = await getDefaultShell()
					.then((result) => result.shell)
					.catch(() => "/bin/zsh");
				const cwd = root ?? "/";
				const result = await spawnTerminal({
					cwd,
					command: shell,
					args: ["-lc", buildCodeRabbitLoginShellCommand()],
					cols: 96,
					rows: 24,
					ptyOwnerKey: `coderabbit-connect:${root ?? "global"}`,
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
					error instanceof Error ? error.message : t("settings.codeRabbit.bootError");
				setBootError(message);
				appendOutput(`\r\n${message}\r\n`);
			}
		})();

		return () => {
			cancelled = true;
		};
	}, [appendOutput, flushPendingOutput, open, root, t]);

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
				className="w-[640px] max-w-[calc(100vw-4rem)] gap-0 overflow-hidden p-0 sm:max-w-[640px]"
			>
				<DialogTitle className="sr-only">
					{t("settings.codeRabbit.dialogTitle")}
				</DialogTitle>
				<DialogDescription className="sr-only">
					{t("settings.codeRabbit.dialogDescription")}
				</DialogDescription>
				<header className="flex h-10 items-center gap-2 border-b border-border/55 px-3">
					<div className="flex min-w-0 items-center gap-1.5 text-[12px] font-medium text-foreground">
						<Rabbit className="size-3.5 shrink-0 text-muted-foreground" strokeWidth={1.8} />
						<span>{t("settings.codeRabbit.dialogTitle")}</span>
						<span className="truncate text-muted-foreground/80">
							· {t("settings.codeRabbit.loginSubtitle")}
						</span>
					</div>
					<div className="ml-auto">
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={() => handleOpenChange(false)}
							aria-label={t("inspector.gitConfirmation.cancel")}
							className="gap-1.5 px-2 text-muted-foreground hover:text-foreground"
						>
							<ShortcutDisplay hotkey="Escape" />
							<X className="size-3.5" strokeWidth={1.8} />
						</Button>
					</div>
				</header>
				<div className="bg-card">
					<TerminalOutput
						terminalRef={termRef}
						className="h-[360px]"
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
