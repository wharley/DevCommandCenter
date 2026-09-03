import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	ChevronLeft,
	ChevronRight,
	CircleCheckBig,
	CircleQuestionMark,
	FolderGit2,
	FolderOpen,
	GitBranch,
	Link2,
	MessageSquareText,
	PanelRight,
	SquareTerminal,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { openGithubCliAuthTerminal } from "@/lib/github-cli";
import { OnboardingMockup } from "./mockup/OnboardingMockup";
import {
	getNextOnboardingStep,
	getPreviousOnboardingStep,
	isLastOnboardingStep,
	onboardingSteps,
	type OnboardingStep,
} from "./OnboardingWizard.logic";

type OnboardingWizardProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onComplete: () => void;
	/** Last step offers to finish straight into the Help panel. */
	onOpenHelp?: () => void;
};

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
}: {
	title: string;
	body: string;
	icon: ReactNode;
}) {
	return (
		<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
			<div className="flex items-center gap-2">
				<div className="flex size-8 items-center justify-center rounded-lg border border-border/60 bg-background/80 text-muted-foreground">
					{icon}
				</div>
				<p className="text-[12px] font-medium text-foreground">{title}</p>
			</div>
			<p className="mt-2 text-[12px] leading-relaxed text-muted-foreground">{body}</p>
		</div>
	);
}

function WizardPanel({
	step,
	index,
	activeIndex,
}: {
	step: OnboardingStep;
	index: number;
	activeIndex: number;
}) {
	const { t } = useTranslation("common");
	const active = index === activeIndex;
	const behind = index < activeIndex;
	const motionClass = active
		? "translate-y-0 scale-100 opacity-100"
		: behind
			? "-translate-y-6 scale-[0.985] opacity-0"
			: "translate-y-6 scale-[0.985] opacity-0";

	return (
		<div
			className={cn(
				"absolute inset-0 z-20 flex translate-y-6 items-center justify-center px-4 sm:translate-y-8 sm:px-6",
				!active && "pointer-events-none",
			)}
			aria-hidden={!active}
		>
			<Card
				className={cn(
					"mx-auto max-h-[calc(100dvh-14rem)] w-[min(640px,calc(100vw-2rem))] overflow-y-auto border-border/60 bg-background/90 shadow-[0_24px_90px_rgba(0,0,0,0.18)] transition-transform duration-1000 ease-[cubic-bezier(.22,.82,.2,1)]",
					motionClass,
				)}
				data-step={step}
			>
				<CardContent className="space-y-5 p-6">
					<div className="space-y-2">
						<Badge variant="outline" className="h-7 px-2.5 text-[11px] font-normal">
							{index + 1} / {onboardingSteps.length}
						</Badge>
						<h2 className="text-[24px] font-semibold tracking-[-0.03em] text-foreground">
							{t(`onboarding.steps.${step}.title`)}
						</h2>
					</div>

					<p className="max-w-2xl text-[14px] leading-6 text-muted-foreground">
						{t(`onboarding.steps.${step}.body`)}
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

					{step === "project" ? (
						<div className="grid gap-3">
							<div className="grid gap-3 sm:grid-cols-2">
								<DetailCard
									title={t("onboarding.project.open.title")}
									body={t("onboarding.project.open.body")}
									icon={<FolderOpen className="size-4" />}
								/>
								<DetailCard
									title={t("onboarding.project.clone.title")}
									body={t("onboarding.project.clone.body")}
									icon={<Link2 className="size-4" />}
								/>
							</div>
							<div className="flex flex-col items-stretch gap-3 rounded-xl border border-border/60 bg-sidebar/40 p-3 sm:flex-row sm:items-center sm:justify-between">
								<div className="min-w-0">
									<p className="text-[12px] font-medium text-foreground">
										{t("onboarding.project.cliTitle")}
									</p>
									<p className="mt-0.5 text-[12px] leading-relaxed text-muted-foreground">
										{t("onboarding.project.cliBody")}
									</p>
								</div>
								<Button
									type="button"
									variant="outline"
									size="sm"
									className="h-8 self-start rounded-[9px] px-2.5 text-[12px] sm:self-auto"
									onClick={async () => {
										const result = await openGithubCliAuthTerminal();
										if (result.success) {
											toast.success(t("onboarding.project.cliOpened"));
											return;
										}
										toast.error(result.error ?? t("onboarding.project.cliFailed"));
									}}
								>
									{t("onboarding.project.cliButton")}
								</Button>
							</div>
						</div>
					) : null}

					{step === "task" ? (
						<div className="grid gap-3 sm:grid-cols-3">
							<DetailCard
								title={t("onboarding.task.worktree.title")}
								body={t("onboarding.task.worktree.body")}
								icon={<FolderGit2 className="size-4" />}
							/>
							<DetailCard
								title={t("onboarding.task.branch.title")}
								body={t("onboarding.task.branch.body")}
								icon={<GitBranch className="size-4" />}
							/>
							<DetailCard
								title={t("onboarding.task.done.title")}
								body={t("onboarding.task.done.body")}
								icon={<CircleCheckBig className="size-4" />}
							/>
						</div>
					) : null}

					{step === "workbench" ? (
						<div className="grid gap-3">
							<div className="grid gap-3 sm:grid-cols-3">
								<DetailCard
									title={t("onboarding.workbench.chat.title")}
									body={t("onboarding.workbench.chat.body")}
									icon={<MessageSquareText className="size-4" />}
								/>
								<DetailCard
									title={t("onboarding.workbench.terminal.title")}
									body={t("onboarding.workbench.terminal.body")}
									icon={<SquareTerminal className="size-4" />}
								/>
								<DetailCard
									title={t("onboarding.workbench.inspector.title")}
									body={t("onboarding.workbench.inspector.body")}
									icon={<PanelRight className="size-4" />}
								/>
							</div>
							<div className="flex items-start gap-3 rounded-xl border border-foreground/20 bg-sidebar/40 p-3">
								<div className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-foreground text-background">
									<CircleQuestionMark className="size-4" strokeWidth={1.9} aria-hidden />
								</div>
								<div className="min-w-0">
									<p className="text-[12px] font-medium text-foreground">
										{t("onboarding.workbench.helpTitle")}
									</p>
									<p className="mt-0.5 text-[12px] leading-relaxed text-muted-foreground">
										{t("onboarding.workbench.helpBody")}
									</p>
								</div>
							</div>
						</div>
					) : null}
				</CardContent>
			</Card>
		</div>
	);
}

export function OnboardingWizard({
	open,
	onOpenChange,
	onComplete,
	onOpenHelp,
}: OnboardingWizardProps) {
	const { t } = useTranslation("common");
	const [step, setStep] = useState<OnboardingStep>("project");
	const [viewportScale, setViewportScale] = useState(1);
	const viewportRef = useRef<HTMLDivElement | null>(null);

	useEffect(() => {
		if (open) {
			setStep("project");
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
	const lastStep = isLastOnboardingStep(step);

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

	const finish = () => {
		onOpenChange(false);
		onComplete();
	};

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
						<WizardPanel key={panel} step={panel} index={index} activeIndex={activeIndex} />
					))}
				</div>

				<div className="absolute inset-x-0 bottom-6 z-20 flex justify-center">
					<div className="flex w-[min(640px,calc(100vw-2rem))] items-center justify-between gap-3 rounded-2xl border border-border/60 bg-background/90 px-4 py-3 shadow-[0_16px_48px_rgba(0,0,0,0.16)] backdrop-blur">
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
							{!lastStep ? (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									className="h-8 rounded-[9px] px-2.5 text-[12px] text-muted-foreground"
									onClick={finish}
								>
									{t("onboarding.skip")}
								</Button>
							) : null}
						</div>

						<div className="flex items-center gap-2">
							{lastStep ? (
								<>
									{onOpenHelp ? (
										<Button
											type="button"
											variant="outline"
											size="sm"
											className="h-8 gap-1.5 rounded-[9px] px-2.5 text-[12px]"
											onClick={() => {
												finish();
												onOpenHelp();
											}}
										>
											<CircleQuestionMark className="size-3.5" />
											{t("onboarding.workbench.openHelp")}
										</Button>
									) : null}
									<Button
										type="button"
										size="sm"
										className="h-8 gap-1.5 rounded-[9px] px-2.5 text-[12px]"
										onClick={finish}
									>
										{t("onboarding.finish")}
									</Button>
								</>
							) : (
								<Button
									type="button"
									size="sm"
									className="h-8 gap-1.5 rounded-[9px] px-2.5 text-[12px]"
									onClick={() => {
										const next = getNextOnboardingStep(step);
										if (next) {
											setStep(next);
										}
									}}
								>
									{t("onboarding.next")}
									<ChevronRight className="size-3.5" />
								</Button>
							)}
						</div>
					</div>
				</div>
			</div>
		</div>
	);
}
