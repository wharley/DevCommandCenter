import {
	BarChart3,
	CircleQuestionMark,
	FolderOpen,
	Globe,
	Plus,
	Settings2,
	Sparkles,
	SquareTerminal,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import type { OnboardingStep } from "../OnboardingWizard.logic";

/**
 * A quiet, scaled-down sketch of the real shell. Labels come from the same
 * i18n keys the app uses, so the sketch cannot drift into another language
 * or into vocabulary the app no longer has. Each step lifts one region and
 * dims the rest; that highlight is the only thing that moves.
 */
export function OnboardingMockup({ step }: { step: OnboardingStep }) {
	const { t } = useTranslation("common");
	const sidebarLit = step === "project" || step === "task";
	const centerLit = step === "workbench";
	const inspectorLit = step === "workbench";

	return (
		<div className="relative h-full w-full overflow-hidden rounded-[28px] border border-border/60 bg-[linear-gradient(180deg,color-mix(in_oklch,var(--background)_94%,transparent),color-mix(in_oklch,var(--sidebar)_96%,transparent))] p-4">
			<div className="grid h-full grid-cols-[236px_minmax(0,1fr)_252px] gap-3">
				<Region lit={sidebarLit}>
					<div className="flex h-full flex-col">
						<div className="flex items-center justify-between px-1">
							<span className="text-[12px] font-medium text-foreground">{t("sidebar.title")}</span>
							<Plus className="size-3.5 text-muted-foreground" aria-hidden />
						</div>

						<div className="mt-3 space-y-2">
							<ProjectRow name="minha-api" lit={step === "project"} />
							<div className="space-y-1 pl-3">
								<TaskRow
									name="feat/checkout-stripe"
									status={t("sidebar.running")}
									lit={step === "task"}
									tone="running"
								/>
								<TaskRow name="fix/login-500" status={t("sidebar.waiting")} tone="waiting" />
							</div>
							<ProjectRow name="web-app" />
						</div>

						{step === "project" ? (
							<Callout className="mt-4">
								<FolderOpen className="size-3.5" aria-hidden />
								{t("sidebar.openProject")}
							</Callout>
						) : null}
						{step === "task" ? (
							<Callout className="mt-4">
								<Plus className="size-3.5" aria-hidden />
								{t("sidebar.newWorkspace")}
							</Callout>
						) : null}

						<div className="mt-auto flex items-center gap-2 px-1 pt-3 text-muted-foreground">
							<BarChart3 className="size-4" strokeWidth={1.8} aria-hidden />
							<Sparkles className="size-4" strokeWidth={1.8} aria-hidden />
							<Settings2 className="size-4" strokeWidth={1.8} aria-hidden />
							<span
								className={cn(
									"relative flex size-6 items-center justify-center rounded-md",
									step === "workbench"
										? "bg-foreground text-background shadow-[0_0_0_6px_color-mix(in_oklch,var(--foreground)_14%,transparent)]"
										: "",
								)}
							>
								<CircleQuestionMark className="size-4" strokeWidth={1.9} aria-hidden />
							</span>
						</div>
					</div>
				</Region>

				<Region lit={centerLit}>
					<div className="flex h-full min-h-0 flex-col">
						<div className="flex items-center justify-between border-b border-border/50 pb-2">
							<div className="min-w-0">
								<p className="truncate text-[12px] font-medium text-foreground">feat/checkout-stripe</p>
								<p className="text-[11px] text-muted-foreground">minha-api</p>
							</div>
							<div className="flex items-center gap-2 text-muted-foreground">
								<SquareTerminal className="size-4" strokeWidth={1.8} aria-hidden />
								<Globe className="size-4" strokeWidth={1.8} aria-hidden />
							</div>
						</div>

						<div className="mt-3 flex-1 space-y-3 overflow-hidden">
							<Bubble align="end">{t("onboarding.mockup.userMessage")}</Bubble>
							<Bubble align="start">{t("onboarding.mockup.agentMessage")}</Bubble>
						</div>

						<div
							className={cn(
								"mt-3 rounded-xl border border-border/60 bg-background/80 px-3 py-2 text-[12px] text-muted-foreground",
								step === "workbench" && "border-foreground/30",
							)}
						>
							{t("composer.placeholder.default")}
						</div>

						<div
							className={cn(
								"mt-3 rounded-xl border border-border/60 bg-sidebar/60 p-3 font-mono text-[11px] leading-5 text-muted-foreground",
								step === "workbench" && "border-foreground/30",
							)}
						>
							<div className="mb-1 flex items-center gap-2 font-sans text-[11px]">
								<SquareTerminal className="size-3.5" aria-hidden />
								<span>{t("terminalDock.scopes.worktree")}</span>
							</div>
							<p>$ yarn dev</p>
							<p>ready on http://localhost:3000</p>
						</div>
					</div>
				</Region>

				<Region lit={inspectorLit}>
					<div className="flex h-full flex-col">
						<div className="flex gap-1 rounded-lg bg-muted/30 p-1 text-[11px]">
							{[t("inspector.tabs.activity"), t("inspector.tabs.context"), t("inspector.tabs.spec")].map(
								(label, index) => (
									<span
										key={label}
										className={cn(
											"flex-1 rounded-md px-2 py-1 text-center",
											index === 0 ? "bg-background text-foreground" : "text-muted-foreground",
										)}
									>
										{label}
									</span>
								),
							)}
						</div>

						<div className="mt-3 space-y-2">
							<ActivityLine text={t("onboarding.mockup.activityRead")} />
							<ActivityLine text={t("onboarding.mockup.activityEdit")} />
							<ActivityLine text={t("onboarding.mockup.activityTest")} />
						</div>

						<div className="mt-auto">
							<p className="px-1 text-[11px] font-medium text-muted-foreground">{t("inspector.modeDock.git")}</p>
							<div className="mt-2 space-y-1.5">
								<DiffRow path="src/checkout/stripe.ts" added={42} removed={3} />
								<DiffRow path="src/checkout/stripe.test.ts" added={18} removed={0} />
							</div>
						</div>
					</div>
				</Region>
			</div>
		</div>
	);
}

function Region({ lit, children }: { lit: boolean; children: React.ReactNode }) {
	return (
		<div
			className={cn(
				"rounded-2xl border bg-background/80 p-3 transition-all duration-700 ease-[cubic-bezier(.22,.82,.2,1)]",
				lit
					? "border-foreground/25 opacity-100 shadow-[0_18px_60px_rgba(0,0,0,0.14)]"
					: "border-border/50 opacity-45",
			)}
		>
			{children}
		</div>
	);
}

function ProjectRow({ name, lit }: { name: string; lit?: boolean }) {
	return (
		<div
			className={cn(
				"flex items-center gap-2 rounded-lg border px-2.5 py-1.5 text-[12px]",
				lit ? "border-foreground/30 bg-background text-foreground" : "border-border/50 bg-background/60 text-foreground/90",
			)}
		>
			<FolderOpen className="size-3.5 text-muted-foreground" aria-hidden />
			<span className="truncate">{name}</span>
		</div>
	);
}

function TaskRow({
	name,
	status,
	lit,
	tone,
}: {
	name: string;
	status: string;
	lit?: boolean;
	tone: "running" | "waiting";
}) {
	return (
		<div
			className={cn(
				"flex items-center justify-between gap-2 rounded-lg border px-2.5 py-1.5 text-[11px]",
				lit ? "border-foreground/30 bg-background text-foreground" : "border-transparent text-foreground/80",
			)}
		>
			<span className="truncate font-mono">{name}</span>
			<span className="flex shrink-0 items-center gap-1 text-muted-foreground">
				<span
					className={cn(
						"size-1.5 rounded-full",
						tone === "running" ? "bg-emerald-500" : "bg-amber-500",
					)}
				/>
				{status}
			</span>
		</div>
	);
}

function Callout({ className, children }: { className?: string; children: React.ReactNode }) {
	return (
		<div
			className={cn(
				"flex items-center gap-2 rounded-lg border border-foreground/30 bg-foreground px-2.5 py-1.5 text-[12px] font-medium text-background shadow-[0_0_0_6px_color-mix(in_oklch,var(--foreground)_10%,transparent)]",
				className,
			)}
		>
			{children}
		</div>
	);
}

function Bubble({ align, children }: { align: "start" | "end"; children: React.ReactNode }) {
	return (
		<div className={cn("flex", align === "end" ? "justify-end" : "justify-start")}>
			<p
				className={cn(
					"max-w-[78%] rounded-2xl px-3 py-2 text-[12px] leading-5",
					align === "end" ? "bg-foreground/90 text-background" : "bg-muted/40 text-foreground/90",
				)}
			>
				{children}
			</p>
		</div>
	);
}

function ActivityLine({ text }: { text: string }) {
	return (
		<div className="flex items-start gap-2 text-[11px] leading-5 text-foreground/85">
			<span className="mt-2 size-1.5 shrink-0 rounded-full bg-emerald-500" />
			<span>{text}</span>
		</div>
	);
}

function DiffRow({ path, added, removed }: { path: string; added: number; removed: number }) {
	return (
		<div className="flex items-center justify-between gap-2 rounded-lg border border-border/50 bg-background/70 px-2.5 py-1.5 font-mono text-[11px]">
			<span className="truncate text-foreground/90">{path}</span>
			<span className="shrink-0">
				<span className="text-emerald-600 dark:text-emerald-400">+{added}</span>{" "}
				<span className="text-red-600 dark:text-red-400">-{removed}</span>
			</span>
		</div>
	);
}
