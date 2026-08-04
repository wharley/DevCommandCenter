import { ArrowLeft, Check, GitFork, Play, X } from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { TrafficLightSpacer } from "@/components/chrome/traffic-light-spacer";
import { PlanReviewCard } from "@/features/panel/message-components";
import type { ParsedPlanContent } from "@/features/panel/plan-content";
import { shouldIgnoreGlobalShortcutTarget } from "@/features/shortcuts/shortcut-utils";
import { ShortcutDisplay } from "@/features/shortcuts/shortcut-display";
import type { ProviderCatalog } from "@dcc/contracts";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuLabel,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
	DelegationTargetItems,
	type DelegationTargetSelection,
} from "@/features/sessions/DelegationTargetItems";

type WorkspacePlanSurfaceProps = {
	plan: ParsedPlanContent;
	version: number;
	workspacePath: string | null;
	approved: boolean;
	approving: boolean;
	readOnly: boolean;
	needsInput: boolean;
	onApprove: () => void;
	onClose: () => void;
	onRequestRevision: (prompt: string) => void;
	delegationTargets: ProviderCatalog["providers"];
	onDelegate: (selection: DelegationTargetSelection) => void;
	onImplementInNewThread: () => void;
};

export function WorkspacePlanSurface({
	plan,
	version,
	workspacePath,
	approved,
	approving,
	readOnly,
	needsInput,
	onApprove,
	onClose,
	onRequestRevision,
	delegationTargets,
	onDelegate,
	onImplementInNewThread,
}: WorkspacePlanSurfaceProps) {
	const { t } = useTranslation("common");

	useEffect(() => {
		const onKeyDown = (event: KeyboardEvent) => {
			if (event.defaultPrevented || event.key !== "Escape") return;
			if (shouldIgnoreGlobalShortcutTarget(event.target)) return;
			event.preventDefault();
			onClose();
		};

		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [onClose]);

	return (
		<section
			aria-label={t("planSurface.ariaLabel")}
			data-focus-scope="editor"
			className="flex h-full min-h-0 flex-col overflow-hidden bg-background text-foreground"
		>
			<div className="flex h-9 shrink-0 items-center border-b border-border" data-tauri-drag-region>
				<TrafficLightSpacer side="left" width={86} />
				<div className="min-w-0 flex-1" data-tauri-drag-region />
				<div className="min-w-0 truncate px-3 text-[11px] text-muted-foreground">
					{plan.title}
				</div>
				<div className="flex shrink-0 items-center pr-2">
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={onClose}
						aria-label={t("planSurface.close")}
						className="gap-1.5 px-2 text-muted-foreground hover:text-foreground"
					>
						<ShortcutDisplay hotkey="Escape" />
						<X className="size-3.5" strokeWidth={1.8} />
					</Button>
				</div>
			</div>

			<header className="shrink-0 border-b border-border/60 bg-background/95 px-5 py-4 backdrop-blur">
				<div className="mx-auto flex w-full max-w-5xl flex-wrap items-center justify-between gap-4">
					<div className="flex min-w-0 items-start gap-3">
						<Button
							type="button"
							variant="ghost"
							size="icon-sm"
							className="mt-0.5 shrink-0"
							onClick={onClose}
							aria-label={t("planSurface.backToThread")}
						>
							<ArrowLeft className="size-4" aria-hidden />
						</Button>
						<div className="min-w-0">
							<div className="flex flex-wrap items-center gap-2">
								<Badge
									variant={
										readOnly
											? "outline"
											: approved
												? "success"
												: needsInput
													? "outline"
													: "secondary"
									}
								>
									{readOnly
										? t("planSurface.readOnly")
										: approved
										? t("planSurface.approved")
										: needsInput
											? t("planSurface.needsInput")
											: t("planSurface.draft")}
								</Badge>
								<span className="text-[11px] text-muted-foreground">
									{t("planSurface.version", { version })}
								</span>
								<span className="text-muted-foreground/40" aria-hidden>
									·
								</span>
								<span className="text-[11px] text-muted-foreground">
									{t("planSurface.stepCount", { count: plan.steps.length })}
								</span>
							</div>
							<h1 className="mt-1 truncate text-lg font-semibold tracking-tight">
								{plan.title}
							</h1>
							<p className="mt-1 text-xs text-muted-foreground">
								{readOnly
									? t("planSurface.readOnlyHint")
									: approved
									? t("planSurface.approvedHint")
									: needsInput
										? t("planSurface.needsInputHint")
										: t("planSurface.reviewHint")}
							</p>
						</div>
					</div>

					<div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
						{readOnly ? null : approved ? (
							<>
								<Button
									type="button"
									variant="outline"
									size="sm"
									className="gap-1.5"
									onClick={onImplementInNewThread}
								>
									<Play className="size-3.5" aria-hidden />
									{t("planSurface.implementInNewThread")}
								</Button>
								<DropdownMenu>
									<DropdownMenuTrigger asChild>
										<Button
											type="button"
											size="sm"
											className="gap-1.5"
											disabled={delegationTargets.length === 0}
										>
											<GitFork className="size-3.5" aria-hidden />
											{t("planSurface.delegate")}
										</Button>
									</DropdownMenuTrigger>
									<DropdownMenuContent side="bottom" align="end" className="w-72">
										<DropdownMenuLabel>{t("composer.delegate.title")}</DropdownMenuLabel>
										<DelegationTargetItems
											targets={delegationTargets}
											onSelect={onDelegate}
										/>
									</DropdownMenuContent>
								</DropdownMenu>
							</>
						) : needsInput ? (
							<Button
								type="button"
								variant="outline"
								size="sm"
								className="gap-1.5"
								onClick={onClose}
							>
								<ArrowLeft className="size-3.5" aria-hidden />
								{t("planSurface.answerInThread")}
							</Button>
						) : (
							<Button
								type="button"
								size="sm"
								className="gap-1.5"
								onClick={onApprove}
								disabled={approving}
							>
								<Check className="size-3.5" aria-hidden />
								{approving
									? t("planSurface.approving")
									: t("planSurface.approve")}
							</Button>
						)}
					</div>
				</div>
			</header>

			<div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-5 py-6">
				<div className="mx-auto w-full max-w-5xl">
					<PlanReviewCard
						plan={plan}
						workspacePath={workspacePath}
						onRequestRevision={readOnly ? undefined : onRequestRevision}
						documentMode
						className="border-border/60 bg-card/55 shadow-none"
					/>
				</div>
			</div>
		</section>
	);
}
