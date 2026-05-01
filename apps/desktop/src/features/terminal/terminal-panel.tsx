import { useEffect, useRef } from "react";
import { Badge } from "@/components/ui/badge";
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

type TerminalPanelProps = {
	workspaceName: string;
	workspaceBranch: string;
	providerLabel: string | null;
	sessionState: string;
	sessionId: string | null;
};

export function TerminalPanel({
	workspaceName,
	workspaceBranch,
	providerLabel,
	sessionState,
	sessionId,
}: TerminalPanelProps) {
	const terminalRef = useRef<TerminalHandle | null>(null);

	useEffect(() => {
		let cancelled = false;
		let rafId = 0;
		const seedTerminal = () => {
			if (cancelled) {
				return;
			}

			const terminal = terminalRef.current;
			if (!terminal) {
				rafId = requestAnimationFrame(seedTerminal);
				return;
			}

			terminal.clear();
			terminal.write(
				[
					"Dev Command Center terminal scaffold",
					"",
					`workspace: ${workspaceName}`,
					`branch: ${workspaceBranch}`,
					`provider: ${providerLabel ?? "none"}`,
					`session: ${sessionState}`,
					"",
					"Helmor-style xterm surface mounted here.",
					"PTY bridge will attach in the next pass.",
				].join("\r\n") + "\r\n",
			);
			terminal.refit();
		};

		rafId = requestAnimationFrame(seedTerminal);
		return () => {
			cancelled = true;
			cancelAnimationFrame(rafId);
		};
	}, [providerLabel, sessionState, workspaceBranch, workspaceName, sessionId]);

	return (
		<Card className="dcc-terminal">
			<CardHeader>
				<div className="dcc-card__meta-row">
					<div>
						<CardTitle>Terminal surface</CardTitle>
						<CardDescription>
							Helmor-style xterm shell scaffold for the runtime workspace.
						</CardDescription>
					</div>
					<Badge variant="outline">{sessionId ?? "No session"}</Badge>
				</div>
			</CardHeader>
			<CardContent className="dcc-terminal__viewport">
				<TerminalOutput
					terminalRef={terminalRef}
					className="dcc-terminal__surface"
					detectLinks
				/>
				<div className="dcc-terminal__note">
					<span>
						<strong>Surface only.</strong> PTY wiring comes after the shell
						layout is stable.
					</span>
					<Badge variant="outline">{providerLabel ?? "Provider pending"}</Badge>
				</div>
			</CardContent>
		</Card>
	);
}
