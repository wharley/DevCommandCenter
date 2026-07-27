import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
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
	const { t } = useTranslation("common");
	const [submitting, setSubmitting] = useState<"allow" | "deny" | null>(null);
	const resolved = !isLive && typeof behavior === "string" && behavior.length > 0;
	const delegationRequest = useMemo(
		() =>
			isDelegateTaskTool(toolName)
				? parseAgentInitiatedDelegationRequest({ command, description })
				: null,
		[command, description, toolName],
	);
	const behaviorLabel =
		behavior === "allow"
			? t("conversation.permission.allowed")
			: behavior === "deny"
				? t("conversation.permission.denied")
				: t("conversation.permission.pending");

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
					throw new Error(
						t("conversation.permission.delegateMissingInstruction"),
					);
				}
				if (!onDelegateTaskApprove) {
					throw new Error(t("conversation.permission.delegateUnavailable"));
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
					: t("conversation.permission.submitError"),
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
						{t("conversation.permission.label")}
					</p>
					<p className="mt-1 text-sm text-foreground">
						{title ??
							(delegationRequest ? t("delegation.approval.title") : toolName)}
					</p>
				</div>
				{resolved ? (
					<span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
						{behavior === "allow" ? (
							<CheckCircle2 className="size-4 text-emerald-600" aria-hidden />
						) : (
							<Ban className="size-4 text-amber-600" aria-hidden />
						)}
						{behaviorLabel}
					</span>
				) : null}
			</div>

			{description && !delegationRequest ? (
				<p className="mb-3 text-sm text-muted-foreground">{description}</p>
			) : null}

			{delegationRequest ? (
				<div className="space-y-3">
					<div>
						<p className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground/70">
							{t("delegation.approval.instruction")}
						</p>
						<p className="mt-1 whitespace-pre-wrap break-words text-sm text-foreground">
							{delegationRequest.instruction}
						</p>
					</div>
					<div className="grid gap-3 sm:grid-cols-3">
						<div>
							<p className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground/70">
								{t("delegation.approval.mode")}
							</p>
							<p className="mt-1 text-sm text-foreground">
								{t(`inspector.delegations.mode.${delegationRequest.mode}`, {
									defaultValue: delegationRequest.mode,
								})}
							</p>
						</div>
						<div>
							<p className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground/70">
								{t("delegation.approval.target")}
							</p>
							<p className="mt-1 text-sm text-foreground">
								{delegationRequest.targetProviderId ??
									t("delegation.approval.targetAuto")}
							</p>
						</div>
						<div>
							<p className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground/70">
								{t("delegation.approval.context")}
							</p>
							<p className="mt-1 text-sm text-foreground">
								{t(
									`delegation.contextOptions.${delegationRequest.contextPolicy.type}`,
									{ defaultValue: delegationRequest.contextPolicy.type },
								)}
							</p>
						</div>
					</div>
					{command ? (
						<details>
							<summary className="cursor-pointer text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground/70">
								{t("delegation.approval.payload")}
							</summary>
							<pre className="mt-1 overflow-x-auto rounded-xl bg-background/70 px-3 py-2 text-xs text-foreground">
								<code>{command}</code>
							</pre>
						</details>
					) : null}
				</div>
			) : (
				<div className="space-y-2">
					<p className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground/70">
						{t("conversation.permission.tool")}
					</p>
					<p className="text-sm text-foreground">{toolName}</p>
					{command ? (
						<div>
							<p className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground/70">
								{t("conversation.permission.command")}
							</p>
							<pre className="mt-1 overflow-x-auto rounded-xl bg-background/70 px-3 py-2 text-xs text-foreground">
								<code>{command}</code>
							</pre>
						</div>
					) : null}
					{file ? (
						<div>
							<p className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground/70">
								{t("conversation.permission.file")}
							</p>
							<p className="mt-1 break-all font-mono text-xs text-foreground">{file}</p>
						</div>
					) : null}
				</div>
			)}

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
						{t("conversation.permission.deny")}
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
						{t("conversation.permission.allow")}
					</Button>
				</div>
			) : null}
		</div>
	);
}
