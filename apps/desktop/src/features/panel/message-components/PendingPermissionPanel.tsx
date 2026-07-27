import { ShieldAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AgentInitiatedDelegationRequest } from "@/features/sessions/agent-delegation-request";
import type { PendingPermissionRequest } from "../pending-permissions";
import { ApprovalCard } from "./ApprovalCard";

type PendingPermissionPanelProps = {
	sessionId: string;
	requests: PendingPermissionRequest[];
	onDelegateTaskApprove?: (
		request: AgentInitiatedDelegationRequest,
	) => Promise<void>;
};

export function PendingPermissionPanel({
	sessionId,
	requests,
	onDelegateTaskApprove,
}: PendingPermissionPanelProps) {
	const { t } = useTranslation("common");

	if (requests.length === 0) {
		return null;
	}

	return (
		<section
			aria-live="assertive"
			aria-label={t("conversation.permission.required")}
			className="shrink-0 border-t border-amber-500/30 bg-amber-500/[0.06] px-3 py-3 sm:px-4"
		>
			<div className="mx-auto mb-2 flex max-w-3xl items-start gap-2.5 text-amber-900 dark:text-amber-100">
				<ShieldAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
				<div className="min-w-0 flex-1">
					<p className="text-sm font-semibold">
						{t("conversation.permission.required")}
					</p>
					<p className="mt-0.5 text-xs text-amber-800/80 dark:text-amber-200/75">
						{t("conversation.permission.waiting")}
					</p>
				</div>
				<span className="shrink-0 rounded-full border border-amber-500/25 bg-background/70 px-2 py-0.5 text-[11px] font-medium">
					{t("conversation.permission.pendingCount", {
						count: requests.length,
					})}
				</span>
			</div>
			<div className="mx-auto flex max-h-[min(42vh,24rem)] max-w-3xl flex-col gap-2 overflow-y-auto overscroll-contain pr-1">
				{requests.map((request) => (
					<ApprovalCard
						key={request.id}
						sessionId={sessionId}
						requestId={request.id}
						toolName={request.toolName}
						title={request.title}
						description={request.description}
						command={request.command}
						file={request.file}
						behavior={request.behavior}
						isLive
						onDelegateTaskApprove={onDelegateTaskApprove}
					/>
				))}
			</div>
		</section>
	);
}
