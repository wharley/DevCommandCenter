import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	CheckCircle2,
	ChevronLeft,
	ChevronRight,
	Command,
	GitBranch,
	ListTree,
	MessageSquare,
	Sparkles,
	Wand2,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { ConductorOnboarding } from "@/components/ConductorOnboarding";
import { cn } from "@/lib/utils";
import { DEFAULT_SLASH_COMMANDS } from "@/features/composer/default-slash-commands";
import { openGithubCliAuthTerminal } from "@/lib/github-cli";
import { OnboardingMockup } from "./mockup/OnboardingMockup";
import {
	futureOnboardingSteps,
	getNextOnboardingStep,
	getPreviousOnboardingStep,
	onboardingSteps,
	type OnboardingStep,
} from "./OnboardingWizard.logic";

type OnboardingWizardProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onComplete: () => void;
};

const ONBOARDING_COMPLETE_KEY = "dcc.onboarding.complete";

function OnboardingDragBar() {
	return (
		<div className="absolute inset-x-0 top-0 z-20 flex h-11 items-center">
			<div className="w-[94px] shrink-0" />
			<div data-tauri-drag-region className="h-full flex-1" />
			<div className="w-[140px] shrink-0" />
		</div>
	);
}

function StepChip({
	label,
	active,
	done,
}: {
	label: string;
	active?: boolean;
	done?: boolean;
}) {
	return (
		<div
			className={cn(
				"flex items-center gap-2 rounded-full border px-3 py-1 text-[11px] transition-colors",
				active
					? "border-foreground/20 bg-background text-foreground"
					: done
						? "border-border/60 bg-muted/30 text-muted-foreground"
						: "border-border/40 bg-transparent text-muted-foreground/70",
			)}
		>
			<span className="size-1.5 rounded-full bg-current" />
			{label}
		</div>
	);
}

function DetailCard({
	title,
	body,
	icon,
	badge,
}: {
	title: string;
	body: string;
	icon: ReactNode;
	badge?: string;
}) {
	return (
		<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
			<div className="flex items-start justify-between gap-3">
				<div className="flex items-center gap-2">
					<div className="flex size-8 items-center justify-center rounded-lg border border-border/60 bg-background/80 text-muted-foreground">
						{icon}
					</div>
					<p className="text-[12px] font-medium text-foreground">{title}</p>
				</div>
				{badge ? (
					<Badge variant="outline" className="h-6 px-2 text-[10px] font-normal">
						{badge}
					</Badge>
				) : null}
			</div>
			<p className="mt-2 text-[12px] leading-relaxed text-muted-foreground">{body}</p>
		</div>
	);
}

function CommandCard({
	command,
	description,
}: {
	command: string;
	description: string;
}) {
	return (
		<div className="rounded-xl border border-border/60 bg-muted/15 p-3">
			<p className="font-mono text-[12px] font-medium text-foreground">/{command}</p>
			<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
				{description}
			</p>
		</div>
	);
}

function WizardPanel({
	step,
	index,
	activeIndex,
	onPreviewConductor,
}: {
	step: OnboardingStep;
	index: number;
	activeIndex: number;
	onPreviewConductor: () => void;
}) {
	const { t } = useTranslation("common");
	const title = t(`onboarding.steps.${step}.title`);
	const body = t(`onboarding.steps.${step}.body`);
	const slashCommands = useMemo(
		() =>
			DEFAULT_SLASH_COMMANDS.map((command) => ({
				name: command.name,
				description: t(`onboarding.slashCommands.items.${command.name}`),
			})),
		[t],
	);
	const active = index === activeIndex;
	const behind = index < activeIndex;
	const motionClass = active
		? "translate-y-0 scale-100 opacity-100"
		: behind
			? "-translate-y-6 scale-[0.985] opacity-0"
			: "translate-y-6 scale-[0.985] opacity-0";

	return (
		<div className="absolute inset-0 z-20 flex translate-y-6 items-center justify-center px-4 sm:translate-y-8 sm:px-6">
			<Card
				className={cn(
					"mx-auto max-h-[calc(100dvh-14rem)] w-[min(680px,calc(100vw-2rem))] overflow-y-auto border-border/60 bg-background/90 shadow-[0_24px_90px_rgba(0,0,0,0.18)] transition-transform duration-1000 ease-[cubic-bezier(.22,.82,.2,1)]",
					motionClass,
				)}
				data-step={step}
			>
				<CardContent className="space-y-5 p-6">
					<div className="flex items-center justify-between gap-4">
						<div className="space-y-2">
							<Badge variant="outline" className="h-7 px-2.5 text-[11px] font-normal">
								{String(index + 1).padStart(2, "0")} / {String(onboardingSteps.length).padStart(2, "0")}
							</Badge>
							<h2 className="text-[24px] font-semibold tracking-[-0.03em] text-foreground">
								{title}
							</h2>
						</div>
						<Sparkles className="size-5 shrink-0 text-muted-foreground" />
					</div>

					<p className="max-w-2xl text-[14px] leading-6 text-muted-foreground">
						{body}
					</p>

					<div className="flex flex-wrap gap-2">
						{onboardingSteps.map((chipStep, chipIndex) => (
							<StepChip
								key={chipStep}
								label={t(`onboarding.chips.${chipStep}`)}
								done={chipIndex < index}
								active={step === chipStep}
							/>
						))}
					</div>

					{step === "intro" ? (
						<div className="grid gap-3 sm:grid-cols-2">
							<DetailCard
								title={t("onboarding.introCards.workspace.title")}
								body={t("onboarding.introCards.workspace.body")}
								icon={<GitBranch className="size-4" />}
							/>
							<DetailCard
								title={t("onboarding.introCards.thread.title")}
								body={t("onboarding.introCards.thread.body")}
								icon={<MessageSquare className="size-4" />}
							/>
						</div>
					) : null}

					{step === "workflows" ? (
						<div className="grid gap-3">
							<div className="grid gap-3 sm:grid-cols-3">
								<DetailCard
									title={t("onboarding.workflows.quickChat.title")}
									body={t("onboarding.workflows.quickChat.body")}
									icon={<MessageSquare className="size-4" />}
									badge={t("onboarding.workflows.quickChat.badge")}
								/>
								<DetailCard
									title={t("onboarding.workflows.planMode.title")}
									body={t("onboarding.workflows.planMode.body")}
									icon={<ListTree className="size-4" />}
									badge={t("onboarding.workflows.planMode.badge")}
								/>
								<DetailCard
									title={t("onboarding.workflows.sdd.title")}
									body={t("onboarding.workflows.sdd.body")}
									icon={<GitBranch className="size-4" />}
									badge={t("onboarding.workflows.sdd.badge")}
								/>
							</div>
							<div className="rounded-xl border border-border/60 bg-sidebar/40 p-3">
								<p className="text-[12px] font-medium text-foreground">
									{t("onboarding.workflows.sddFlowTitle")}
								</p>
								<div className="mt-2 flex flex-wrap gap-1.5">
									{[
										t("onboarding.workflows.sddFlow.spec"),
										t("onboarding.workflows.sddFlow.plan"),
										t("onboarding.workflows.sddFlow.validate"),
										t("onboarding.workflows.sddFlow.continue"),
									].map((item) => (
										<Badge
											key={item}
											variant="outline"
											className="h-7 px-2.5 text-[11px] font-normal"
										>
											{item}
										</Badge>
									))}
								</div>
								<p className="mt-2 text-[12px] leading-relaxed text-muted-foreground">
									{t("onboarding.workflows.sddNote")}
								</p>
							</div>
						</div>
					) : null}

					{step === "slashCommands" ? (
						<div className="grid gap-3">
							<div className="rounded-xl border border-border/60 bg-sidebar/40 p-3">
								<div className="flex items-center gap-2">
									<Command className="size-4 text-muted-foreground" />
									<p className="text-[12px] font-medium text-foreground">
										{t("onboarding.slashCommands.title")}
									</p>
								</div>
								<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
									{t("onboarding.slashCommands.body")}
								</p>
							</div>
							<div className="grid gap-3 sm:grid-cols-2">
								{slashCommands.map((command) => (
									<CommandCard
										key={command.name}
										command={command.name}
										description={command.description}
									/>
								))}
							</div>
						</div>
					) : null}

					{step === "agents" ? (
						<div className="grid gap-3 sm:grid-cols-3">
							<DetailCard
								title={t("onboarding.agentsCards.activity.title")}
								body={t("onboarding.agentsCards.activity.body")}
								icon={<MessageSquare className="size-4" />}
							/>
							<DetailCard
								title={t("onboarding.agentsCards.spec.title")}
								body={t("onboarding.agentsCards.spec.body")}
								icon={<GitBranch className="size-4" />}
							/>
							<DetailCard
								title={t("onboarding.agentsCards.plan.title")}
								body={t("onboarding.agentsCards.plan.body")}
								icon={<ListTree className="size-4" />}
							/>
						</div>
					) : null}

					{step === "repoImport" ? (
						<div className="grid gap-3 sm:grid-cols-2">
							<DetailCard
								title={t("onboarding.repoCards.openLocal.title")}
								body={t("onboarding.repoCards.openLocal.body")}
								icon={<GitBranch className="size-4" />}
							/>
							<DetailCard
								title={t("onboarding.repoCards.cloneRemote.title")}
								body={t("onboarding.repoCards.cloneRemote.body")}
								icon={<CheckCircle2 className="size-4" />}
							/>
						</div>
					) : null}

					{step === "completeTransition" ? (
						<div className="grid gap-3 sm:grid-cols-2">
							<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
								<p className="text-[12px] font-medium text-foreground">
									{t("onboarding.phase4Title")}
								</p>
								<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
									{t("onboarding.phase4Body")}
								</p>
							</div>
							<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
								<p className="text-[12px] font-medium text-foreground">
									{t("onboarding.laterSteps")}
								</p>
								<div className="mt-2 flex flex-wrap gap-1.5">
									{futureOnboardingSteps.map((item) => (
										<Badge
											key={item}
											variant="outline"
											className="h-7 px-2.5 text-[11px] font-normal"
										>
											{item}
										</Badge>
									))}
								</div>
							</div>
						</div>
					) : null}

					{step === "repoImport" ? (
						<div className="flex items-center justify-between gap-3 rounded-xl border border-border/60 bg-sidebar/40 p-3">
							<div className="min-w-0">
								<p className="text-[12px] font-medium text-foreground">
									{t("onboarding.repoCliTitle")}
								</p>
								<p className="mt-0.5 text-[12px] leading-relaxed text-muted-foreground">
									{t("onboarding.repoCliBody")}
								</p>
							</div>
							<Button
								type="button"
								variant="outline"
								size="sm"
								className="h-8 rounded-[9px] px-2.5 text-[12px]"
								onClick={async () => {
									const result = await openGithubCliAuthTerminal();
									if (result.success) {
										toast.success(t("onboarding.repoCliOpened"));
										return;
									}
									toast.error(
										result.error ?? t("onboarding.repoCliFailed"),
									);
								}}
							>
								{t("onboarding.repoCliButton")}
							</Button>
						</div>
					) : null}

					<div className="flex items-center justify-between gap-3 rounded-xl border border-border/60 bg-sidebar/40 p-3">
						<div className="min-w-0">
							<p className="text-[12px] font-medium text-foreground">{t("onboarding.conductorPreviewTitle")}</p>
							<p className="mt-0.5 text-[12px] leading-relaxed text-muted-foreground">
								{t("onboarding.conductorPreviewBody")}
							</p>
						</div>
						<Button
							type="button"
							variant="outline"
							size="sm"
							className="h-8 rounded-[9px] px-2.5 text-[12px]"
							onClick={onPreviewConductor}
						>
							{t("onboarding.previewConductor")}
						</Button>
					</div>
				</CardContent>
			</Card>
		</div>
	);
}

export function OnboardingWizard({
	open,
	onOpenChange,
	onComplete,
}: OnboardingWizardProps) {
	const { t } = useTranslation("common");
	const [step, setStep] = useState<OnboardingStep>("intro");
	const [viewportScale, setViewportScale] = useState(1);
	const [showConductorPreview, setShowConductorPreview] = useState(false);
	const viewportRef = useRef<HTMLDivElement | null>(null);

	useEffect(() => {
		if (open) {
			setStep("intro");
		}
	}, [open]);

	useEffect(() => {
		const viewport = viewportRef.current;
		if (!viewport) {
			return;
		}

		const updateScale = () => {
			const width = viewport.clientWidth;
			const height = viewport.clientHeight;
			const nextScale = Math.min(width / 1300, height / 900, 1);
			setViewportScale(Number.isFinite(nextScale) ? nextScale : 1);
		};

		updateScale();
		const observer = new ResizeObserver(updateScale);
		observer.observe(viewport);
		return () => observer.disconnect();
	}, [open]);

	const activeIndex = useMemo(() => onboardingSteps.indexOf(step), [step]);
	const canGoBack = activeIndex > 0;
	const canGoNext = activeIndex < onboardingSteps.length - 1;

	useEffect(() => {
		const onKeyDown = (event: KeyboardEvent) => {
			if (!open) {
				return;
			}

			if (event.key === "Escape") {
				onOpenChange(false);
			}
		};

		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [open, onOpenChange]);

	if (!open) {
		return null;
	}

	return (
		<div className="fixed inset-0 z-[90] overflow-hidden bg-background text-foreground">
			<div
				aria-hidden="true"
				className="pointer-events-none absolute inset-0 opacity-[0.08]"
				style={{
					backgroundImage: [
						"linear-gradient(var(--foreground) 1px, transparent 1px)",
						"linear-gradient(90deg, var(--foreground) 1px, transparent 1px)",
					].join(", "),
					backgroundSize: "72px 72px",
					maskImage: "radial-gradient(circle at center, black, transparent 82%)",
				}}
			/>
			<div className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_50%_22%,color-mix(in_oklch,var(--workspace-pr-open-accent)_18%,transparent),transparent_36%),linear-gradient(180deg,transparent,transparent_60%,color-mix(in_oklch,var(--sidebar)_88%,transparent))]" />

			<OnboardingDragBar />

			<div
				ref={viewportRef}
				className="absolute inset-x-0 bottom-0 top-11 overflow-hidden px-4 pb-6 pt-4 sm:px-6"
			>
					<div
						className="pointer-events-none absolute inset-x-0 top-0 flex justify-center"
						style={{ transform: `scale(${viewportScale})`, transformOrigin: "top center" }}
					>
						<div className="h-[900px] w-[1300px]">
							<OnboardingMockup step={step} />
						</div>
					</div>

				<div className="absolute inset-x-0 top-0 bottom-24">
					{onboardingSteps.map((panel, index) => (
						<WizardPanel
							key={panel}
							step={panel}
							index={index}
							activeIndex={activeIndex}
							onPreviewConductor={() => setShowConductorPreview(true)}
						/>
					))}
				</div>

				<div className="absolute inset-x-0 bottom-6 z-20 flex justify-center">
					<div className="flex w-[min(680px,calc(100vw-2rem))] items-center justify-between gap-3 rounded-2xl border border-border/60 bg-background/90 px-4 py-3 shadow-[0_16px_48px_rgba(0,0,0,0.16)] backdrop-blur">
						<div className="flex items-center gap-2">
							<Button
								type="button"
								variant="outline"
								size="sm"
								className="h-8 gap-1.5 rounded-[9px] px-2.5 text-[12px]"
								disabled={!canGoBack}
								onClick={() => {
									const previous = getPreviousOnboardingStep(step);
									if (previous) {
										setStep(previous);
									}
								}}
							>
								<ChevronLeft className="size-3.5" />
								{t("onboarding.back")}
							</Button>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								className="h-8 rounded-[9px] px-2.5 text-[12px] text-muted-foreground"
								onClick={() => {
									onOpenChange(false);
									onComplete();
								}}
							>
								{t("onboarding.skip")}
							</Button>
						</div>

						<div className="flex items-center gap-2">
							<Button
								type="button"
								variant="outline"
								size="sm"
								className="h-8 gap-1.5 rounded-[9px] px-2.5 text-[12px]"
								onClick={() => {
									const next = getNextOnboardingStep(step);
									if (next) {
										setStep(next);
									}
								}}
								disabled={!canGoNext}
							>
								{t("onboarding.next")}
								<ChevronRight className="size-3.5" />
							</Button>
							<Button
								type="button"
								size="sm"
								className="h-8 gap-1.5 rounded-[9px] px-2.5 text-[12px]"
								onClick={() => {
									onOpenChange(false);
									onComplete();
								}}
							>
								<Wand2 className="size-3.5" />
								{t("onboarding.finish")}
							</Button>
						</div>
					</div>
				</div>
			</div>

			<ConductorOnboarding
				open={showConductorPreview}
				onOpenChange={setShowConductorPreview}
			/>
		</div>
	);
}
