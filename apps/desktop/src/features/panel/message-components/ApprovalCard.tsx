import { useState } from "react";
import { Ban, CheckCircle2, LoaderCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { respondToPermissionRequest } from "@/lib/session-api";
import { toast } from "sonner";
import {
	isDelegateTaskTool,
	parseAgentInitiatedDelegationRequest,
	type AgentInitiatedDelegationRequest,
} from "@/features/sessions/agent-delegation-request";

type ApprovalCardProps = {
	sessionId: string | null;
	requestId: string;
	toolName: string;
	title?: string;
	description?: string;
	command?: string;
	file?: string;
	behavior?: string;
	isLive: boolean;
	onDelegateTaskApprove?: (request: AgentInitiatedDelegationRequest) => Promise<void>;
};

function behaviorLabel(behavior: string | undefined) {
	if (behavior === "allow") {
		return "Allowed";
	}
	if (behavior === "deny") {
		return "Denied";
	}
	return "Pending";
}

export function ApprovalCard({
	sessionId,
	requestId,
	toolName,
	title,
	description,
	command,
	file,
	behavior,
	isLive,
	onDelegateTaskApprove,
}: ApprovalCardProps) {
	const [submitting, setSubmitting] = useState<"allow" | "deny" | null>(null);
	const resolved = !isLive && typeof behavior === "string" && behavior.length > 0;

	async function submit(nextBehavior: "allow" | "deny") {
		if (!sessionId) {
			return;
		}
		setSubmitting(nextBehavior);
		try {
			if (nextBehavior === "allow" && isDelegateTaskTool(toolName)) {
				const request = parseAgentInitiatedDelegationRequest({
					command,
					description,
				});
				if (!request) {
					throw new Error("delegate_task request is missing an instruction.");
				}
				if (!onDelegateTaskApprove) {
					throw new Error("delegate_task is unavailable in this context.");
				}
				await onDelegateTaskApprove(request);
			}
			await respondToPermissionRequest({
				sessionId,
				requestId,
				behavior: nextBehavior,
			});
		} catch (error) {
			toast.error(
				error instanceof Error
					? error.message
					: "Unable to send the permission decision to the agent.",
			);
		} finally {
			setSubmitting(null);
		}
	}

	return (
		<div className="rounded-2xl border border-border/70 bg-card/70 p-4">
			<div className="mb-3 flex items-center justify-between gap-3">
				<div>
					<p className="text-[11px] font-medium uppercase tracking-[0.22em] text-muted-foreground/80">
						Approval
					</p>
					<p className="mt-1 text-sm text-foreground">
						{title ?? toolName}
					</p>
				</div>
				{resolved ? (
					<span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
						{behavior === "allow" ? (
							<CheckCircle2 className="size-4 text-emerald-600" aria-hidden />
						) : (
							<Ban className="size-4 text-amber-600" aria-hidden />
						)}
						{behaviorLabel(behavior)}
					</span>
				) : null}
			</div>

			{description ? (
				<p className="mb-3 text-sm text-muted-foreground">{description}</p>
			) : null}

			<div className="space-y-2">
				<p className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground/70">
					Tool
				</p>
				<p className="text-sm text-foreground">{toolName}</p>
				{command ? (
					<div>
						<p className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground/70">
							Command
						</p>
						<pre className="mt-1 overflow-x-auto rounded-xl bg-background/70 px-3 py-2 text-xs text-foreground">
							<code>{command}</code>
						</pre>
					</div>
				) : null}
				{file ? (
					<div>
						<p className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground/70">
							File
						</p>
						<p className="mt-1 break-all font-mono text-xs text-foreground">{file}</p>
					</div>
				) : null}
			</div>

			{isLive ? (
				<div className="mt-4 flex items-center justify-end gap-2">
					<Button
						type="button"
						variant="outline"
						size="sm"
						disabled={!sessionId || submitting !== null}
						onClick={() => void submit("deny")}
					>
						{submitting === "deny" ? (
							<LoaderCircle className="size-4 animate-spin" aria-hidden />
						) : null}
						Deny
					</Button>
					<Button
						type="button"
						size="sm"
						disabled={!sessionId || submitting !== null}
						onClick={() => void submit("allow")}
					>
						{submitting === "allow" ? (
							<LoaderCircle className="size-4 animate-spin" aria-hidden />
						) : null}
						Allow
					</Button>
				</div>
			) : null}
		</div>
	);
}
