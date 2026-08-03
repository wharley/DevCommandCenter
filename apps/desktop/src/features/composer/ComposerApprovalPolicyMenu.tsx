import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, ChevronDown, Shield, ShieldAlert, ShieldCheck } from "lucide-react";
import type { ProviderApprovalPolicy } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { composerToolbarTriggerClassName } from "./WorkspaceComposer.logic";

type ComposerApprovalPolicyMenuProps = {
	providerName: string | null;
	supportedPolicies: readonly ProviderApprovalPolicy[];
	selectedPolicy: ProviderApprovalPolicy | null;
	disabled: boolean;
	planMode: boolean;
	onSelect: (policy: ProviderApprovalPolicy) => void;
};

const POLICY_ORDER: ProviderApprovalPolicy[] = ["ask", "auto", "full_access"];

function PolicyIcon({ policy }: { policy: ProviderApprovalPolicy | null }) {
	if (policy === "full_access") return <ShieldAlert className="size-[13px]" strokeWidth={1.8} />;
	if (policy === "auto") return <ShieldCheck className="size-[13px]" strokeWidth={1.8} />;
	return <Shield className="size-[13px]" strokeWidth={1.8} />;
}

export function ComposerApprovalPolicyMenu({
	providerName,
	supportedPolicies,
	selectedPolicy,
	disabled,
	planMode,
	onSelect,
}: ComposerApprovalPolicyMenuProps) {
	const { t } = useTranslation("common");
	const [confirmFullAccess, setConfirmFullAccess] = useState(false);
	const availablePolicies = POLICY_ORDER.filter((policy) =>
		supportedPolicies.includes(policy),
	);

	if (availablePolicies.length === 0 || !selectedPolicy) {
		return (
			<Tooltip>
				<TooltipTrigger asChild>
					<span
						className={cn(
							"flex h-7 shrink-0 cursor-default items-center gap-1 px-1.5 text-[var(--dcc-daily-meta-size)] text-muted-foreground/55",
							disabled && "opacity-45",
						)}
					>
						<Shield className="size-[13px]" strokeWidth={1.8} />
						<span className="dcc-composer-approval-label text-[12px] font-medium leading-4">
							{t("composer.approval.managed")}
						</span>
					</span>
				</TooltipTrigger>
				<TooltipContent side="top">
					{t("composer.approval.managedHint", { provider: providerName ?? "Provider" })}
				</TooltipContent>
			</Tooltip>
		);
	}

	const label = t(`composer.approval.options.${selectedPolicy}.label`);
	const trigger = (
		<DropdownMenuTrigger
			type="button"
			disabled={disabled}
			aria-label={t("composer.approval.open")}
			className={cn(
				`flex h-7 shrink-0 items-center gap-1 ${composerToolbarTriggerClassName}`,
				"px-1.5 text-[var(--dcc-daily-meta-size)]",
				selectedPolicy === "full_access"
					? "bg-orange-500/10 text-orange-500 hover:bg-orange-500/15 hover:text-orange-500"
					: selectedPolicy === "auto"
						? "text-emerald-500 hover:text-emerald-500"
						: "text-muted-foreground",
				disabled && "cursor-not-allowed opacity-45 hover:bg-transparent",
			)}
		>
			<PolicyIcon policy={selectedPolicy} />
			<span className="dcc-composer-approval-label text-[12px] font-medium leading-4 whitespace-nowrap">
				{label}
			</span>
			<ChevronDown className="size-3 opacity-40" strokeWidth={2} />
		</DropdownMenuTrigger>
	);

	return (
		<>
			<DropdownMenu>
				{planMode ? (
					<Tooltip>
						<TooltipTrigger asChild>{trigger}</TooltipTrigger>
						<TooltipContent side="top">{t("composer.approval.planOverride")}</TooltipContent>
					</Tooltip>
				) : (
					trigger
				)}
				<DropdownMenuContent side="top" align="start" sideOffset={4} className="w-80">
					{availablePolicies.map((policy) => (
						<DropdownMenuItem
							key={policy}
							className="items-start gap-2.5 py-2.5"
							onSelect={() => {
								if (policy === "full_access" && selectedPolicy !== "full_access") {
									setConfirmFullAccess(true);
									return;
								}
								onSelect(policy);
							}}
						>
							<span className={cn("mt-0.5", policy === "full_access" && "text-orange-500")}>
								<PolicyIcon policy={policy} />
							</span>
							<span className="min-w-0 flex-1">
								<span className={cn("block", policy === "full_access" && "text-orange-500")}>
									{t(`composer.approval.options.${policy}.label`)}
								</span>
								<span className="mt-0.5 block text-[12px] leading-4 text-muted-foreground">
									{t(`composer.approval.options.${policy}.description`)}
								</span>
							</span>
							{selectedPolicy === policy ? <Check className="mt-0.5 size-4 shrink-0" /> : null}
						</DropdownMenuItem>
					))}
				</DropdownMenuContent>
			</DropdownMenu>

			<Dialog open={confirmFullAccess} onOpenChange={setConfirmFullAccess}>
				<DialogContent showCloseButton={false}>
					<DialogHeader>
						<div className="mb-1 flex size-9 items-center justify-center rounded-full bg-orange-500/10 text-orange-500">
							<ShieldAlert className="size-5" />
						</div>
						<DialogTitle>{t("composer.approval.confirm.title")}</DialogTitle>
						<DialogDescription>{t("composer.approval.confirm.description")}</DialogDescription>
					</DialogHeader>
					<div className="rounded-lg border border-orange-500/20 bg-orange-500/5 px-3 py-2 text-[12px] leading-5 text-muted-foreground">
						{t("composer.approval.confirm.worktreeWarning")}
					</div>
					<DialogFooter>
						<Button type="button" variant="ghost" onClick={() => setConfirmFullAccess(false)}>
							{t("composer.approval.confirm.cancel")}
						</Button>
						<Button
							type="button"
							className="bg-orange-600 text-white hover:bg-orange-700"
							onClick={() => {
								onSelect("full_access");
								setConfirmFullAccess(false);
							}}
						>
							{t("composer.approval.confirm.enable")}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</>
	);
}
