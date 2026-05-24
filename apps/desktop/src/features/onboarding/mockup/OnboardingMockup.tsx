import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import type { OnboardingStep } from "../OnboardingWizard.logic";

function MockWindow({ title }: { title: string }) {
	return (
		<Card className="border-border/60 bg-background/80 shadow-none">
			<CardContent className="space-y-2 p-3">
				<p className="text-[12px] font-medium text-foreground">{title}</p>
				<div className="h-20 rounded-lg border border-border/60 bg-muted/25" />
			</CardContent>
		</Card>
	);
}

function buildMockupState(step: OnboardingStep) {
	switch (step) {
		case "workflows":
			return {
				title: "Choose the right start",
				subtitle: "Prompt directly, ask for a plan, or open the SDD lane from /spec.",
				composerMode: "SDD flow",
				composerPrompt: "/spec Add mission criteria for the onboarding refresh",
				inspectorCards: ["Spec", "Plan", "Validation"],
				slashOpen: false,
				showFlowRail: true,
			};
		case "slashCommands":
			return {
				title: "Slash commands are entry points",
				subtitle: "Typing / opens local actions before you leave the composer.",
				composerMode: "Composer",
				composerPrompt: "/",
				inspectorCards: ["Activity", "Spec", "Plan"],
				slashOpen: true,
				showFlowRail: false,
			};
		case "agents":
			return {
				title: "Inspector and thread stay aligned",
				subtitle: "Activity, spec state, and plan review remain attached to the active workspace.",
				composerMode: "Plan mode",
				composerPrompt: "Inspect the repo and return a concise plan before editing.",
				inspectorCards: ["Activity", "Spec", "Plan"],
				slashOpen: false,
				showFlowRail: false,
			};
		case "repoImport":
			return {
				title: "Import first, then compose",
				subtitle: "Open a local repo or clone from URL and the rest of the workbench wraps around it.",
				composerMode: "Workspace ready",
				composerPrompt: "Open project or clone from URL to begin.",
				inspectorCards: ["Branches", "Session state", "Providers"],
				slashOpen: false,
				showFlowRail: false,
			};
		case "completeTransition":
			return {
				title: "Same shell, no context switch",
				subtitle: "The onboarding exits directly into the same workspace, composer, and inspector layout.",
				composerMode: "Ready",
				composerPrompt: "Create a workspace and start a thread when you are ready.",
				inspectorCards: ["Spec", "Plan", "Git"],
				slashOpen: false,
				showFlowRail: true,
			};
		case "intro":
		default:
			return {
				title: "Compose, inspect, and ship",
				subtitle: "DCC keeps the active workspace, thread, and inspector visible together.",
				composerMode: "Quick chat",
				composerPrompt: "Summarize this repo and suggest the next best action.",
				inspectorCards: ["Inspector", "Session state", "Branches"],
				slashOpen: false,
				showFlowRail: false,
			};
	}
}

export function OnboardingMockup({ step }: { step: OnboardingStep }) {
	const state = buildMockupState(step);

	return (
		<div className="relative h-full w-full overflow-hidden rounded-[28px] border border-border/60 bg-[radial-gradient(circle_at_20%_0%,color-mix(in_oklch,var(--workspace-pr-open-accent)_20%,transparent),transparent_28%),linear-gradient(180deg,color-mix(in_oklch,var(--background)_92%,transparent),color-mix(in_oklch,var(--sidebar)_96%,transparent))] p-4">
			<div className="grid h-full grid-cols-[240px_minmax(0,1fr)_240px] gap-3">
				<div className="flex flex-col gap-3 rounded-2xl border border-border/60 bg-sidebar/80 p-3">
					<Badge variant="outline" className="h-7 w-fit px-2.5 text-[11px] font-normal">
						Workspaces
					</Badge>
					<div className="space-y-2">
						{["Alpha", "Core Refactor", "Provider Swap"].map((item) => (
							<div key={item} className="rounded-lg border border-border/60 bg-background/80 px-3 py-2 text-[12px]">
								{item}
							</div>
						))}
					</div>
					<Button type="button" variant="outline" size="sm" className="mt-auto h-8 text-[12px]">
						Open project
					</Button>
				</div>

				<div className="flex min-h-0 flex-col gap-3 rounded-2xl border border-border/60 bg-background/85 p-4">
					<div className="flex items-center justify-between">
						<div>
							<p className="text-[12px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
								Workbench
							</p>
							<p className="text-[14px] font-medium text-foreground">
								{state.title}
							</p>
							<p className="mt-1 max-w-md text-[12px] leading-5 text-muted-foreground">
								{state.subtitle}
							</p>
						</div>
						<Badge variant="outline">{state.composerMode}</Badge>
					</div>

					<div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_220px]">
						<div className="rounded-2xl border border-border/60 bg-sidebar/70 p-3">
							<div className="flex items-center justify-between">
								<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
									Composer
								</p>
								<div className="flex gap-1.5">
									{["Quick", "Plan", "SDD"].map((item) => (
										<Badge
											key={item}
											variant="outline"
											className="h-6 px-2 text-[10px] font-normal"
										>
											{item}
										</Badge>
									))}
								</div>
							</div>
							<div className="mt-3 rounded-xl border border-border/60 bg-background/85 p-3">
								<p className="font-mono text-[12px] text-foreground">{state.composerPrompt}</p>
							</div>
							{state.slashOpen ? (
								<div className="mt-3 rounded-xl border border-border/60 bg-background/88 p-2.5">
									<div className="grid gap-1.5">
										{[
											"/add-dir",
											"/compact",
											"/spec",
											"/context",
											"/commit",
											"/clear",
										].map((command) => (
											<div
												key={command}
												className="flex items-center justify-between rounded-lg border border-border/60 bg-muted/15 px-2.5 py-2 text-[11px]"
											>
												<span className="font-mono text-foreground">{command}</span>
												<span className="text-muted-foreground">action</span>
											</div>
										))}
									</div>
								</div>
							) : null}
						</div>

						<div className="space-y-3">
							<div className="rounded-2xl border border-border/60 bg-sidebar/60 p-3">
								<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
									Inspector
								</p>
								<div className="mt-2 grid gap-2">
									{state.inspectorCards.map((item) => (
										<div
											key={item}
											className="rounded-xl border border-border/60 bg-background/80 px-3 py-2 text-[12px] text-foreground"
										>
											{item}
										</div>
									))}
								</div>
							</div>

							{state.showFlowRail ? (
								<div className="rounded-2xl border border-border/60 bg-background/85 p-3">
									<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
										Flow
									</p>
									<div className="mt-2 flex flex-wrap gap-1.5">
										{["/spec", "Spec", "Plan", "Validate", "Continue"].map((item) => (
											<Badge
												key={item}
												variant="outline"
												className="h-6 px-2 text-[10px] font-normal"
											>
												{item}
											</Badge>
										))}
									</div>
								</div>
							) : (
								<div className="rounded-2xl border border-border/60 bg-background/85 p-3">
									<p className="text-[12px] text-muted-foreground">
										The composer and thread viewport stay in lockstep with the selected workspace.
									</p>
								</div>
							)}
						</div>
					</div>
				</div>

				<div className="flex flex-col gap-3 rounded-2xl border border-border/60 bg-sidebar/80 p-3">
					{state.inspectorCards.map((title) => (
						<MockWindow key={title} title={title} />
					))}
				</div>
			</div>
		</div>
	);
}
