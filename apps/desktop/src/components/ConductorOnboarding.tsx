import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";

type ConductorOnboardingProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
};

const nodes = [
	{ label: "Workspace", x: 20, y: 34 },
	{ label: "Agents", x: 20, y: 58 },
	{ label: "Tools", x: 20, y: 82 },
];

export function ConductorOnboarding({
	open,
	onOpenChange,
}: ConductorOnboardingProps) {
	if (!open) {
		return null;
	}

	return (
		<div className="fixed inset-0 z-[96] overflow-hidden bg-background text-foreground">
			<div className="absolute inset-0 bg-[radial-gradient(circle_at_center,color-mix(in_oklch,var(--workspace-pr-open-accent)_18%,transparent),transparent_36%),linear-gradient(180deg,transparent,transparent_65%,color-mix(in_oklch,var(--sidebar)_86%,transparent))]" />
			<div className="absolute inset-x-0 top-0 z-20 flex h-11 items-center">
				<div className="w-[94px] shrink-0" />
				<div className="h-full flex-1" />
				<div className="w-[140px] shrink-0" />
			</div>

			<div className="absolute inset-x-0 top-11 bottom-0 flex items-center justify-center px-4 py-6">
				<div className="relative w-[min(980px,calc(100vw-2rem))] overflow-hidden rounded-[28px] border border-border/60 bg-background/92 shadow-[0_28px_120px_rgba(0,0,0,0.24)]">
					<div className="flex items-center justify-between border-b border-border/40 px-5 py-4">
						<div className="space-y-1">
							<Badge variant="outline" className="h-7 px-2.5 text-[11px] font-normal">
								Conductor
							</Badge>
							<p className="text-[15px] font-semibold tracking-[-0.02em]">
								Multi-agent handoff preview
							</p>
						</div>
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							aria-label="Close Conductor preview"
							onClick={() => onOpenChange(false)}
						>
							<X className="size-4" />
						</Button>
					</div>

					<div className="grid min-h-0 grid-cols-[220px_minmax(0,1fr)_220px]">
						<div className="space-y-3 border-r border-border/40 bg-sidebar/75 p-4">
							<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
								Incoming work
							</p>
							<div className="space-y-2">
								{nodes.map((node) => (
									<div
										key={node.label}
										className="rounded-xl border border-border/60 bg-background/80 px-3 py-2 text-[12px]"
									>
										{node.label}
									</div>
								))}
							</div>
						</div>

						<div className="relative min-h-[420px] overflow-hidden bg-[radial-gradient(circle_at_center,color-mix(in_oklch,var(--workspace-pr-open-accent)_14%,transparent),transparent_48%)] p-4">
							<svg
								viewBox="0 0 100 100"
								className="pointer-events-none absolute inset-0 h-full w-full"
								aria-hidden
							>
								<defs>
									<linearGradient id="conductorBeam" x1="0%" y1="0%" x2="100%" y2="0%">
										<stop offset="0%" stopColor="color-mix(in_oklch,var(--workspace-pr-open-accent)_12%,transparent)" />
										<stop offset="50%" stopColor="var(--workspace-pr-open-accent)" />
										<stop offset="100%" stopColor="color-mix(in_oklch,var(--workspace-pr-open-accent)_12%,transparent)" />
									</linearGradient>
								</defs>
								{[
									"M 20 34 C 36 34, 42 50, 50 50",
									"M 20 58 C 36 58, 42 52, 50 50",
									"M 20 82 C 38 82, 42 56, 50 50",
									"M 50 50 C 62 48, 74 42, 80 34",
									"M 50 50 C 62 50, 74 50, 80 58",
									"M 50 50 C 62 52, 74 60, 80 82",
								].map((path, index) => (
									<path
										key={path}
										d={path}
										fill="none"
										stroke="url(#conductorBeam)"
										strokeWidth="1.25"
										strokeLinecap="round"
										strokeDasharray="3 5"
										className={cn("opacity-70 motion-safe:animate-[shine_5s_infinite_linear]")}
										style={{ animationDelay: `${index * 180}ms` }}
									/>
								))}
							</svg>

							<div className="relative z-10 flex h-full flex-col items-center justify-center gap-4">
								<div className="flex size-24 items-center justify-center rounded-full border border-border/60 bg-background shadow-[0_0_0_1px_color-mix(in_oklch,var(--workspace-pr-open-accent)_22%,transparent),0_0_50px_color-mix(in_oklch,var(--workspace-pr-open-accent)_24%,transparent)]">
									<div className="flex size-14 items-center justify-center rounded-2xl bg-[color-mix(in_oklch,var(--workspace-pr-open-accent)_16%,var(--background))] text-[14px] font-semibold tracking-[0.08em] text-foreground">
										CD
									</div>
								</div>
								<div className="max-w-md space-y-2 text-center">
									<p className="text-[18px] font-semibold tracking-[-0.02em]">
										Conductor coordinates workspace work and agent handoff.
									</p>
									<p className="text-[13px] leading-6 text-muted-foreground">
										This is a preview-only surface for the optional integration pass. It keeps the visual flow available without forcing backend behavior yet.
									</p>
								</div>
							</div>
						</div>

						<div className="space-y-3 border-l border-border/40 bg-sidebar/75 p-4">
							<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
								Outputs
							</p>
							<div className="space-y-2">
								{["Plan", "Task list", "Workspace handoff"].map((item) => (
									<Card key={item} className="border-border/60 shadow-none">
										<CardContent className="p-3">
											<p className="text-[12px] font-medium text-foreground">{item}</p>
										</CardContent>
									</Card>
								))}
							</div>
						</div>
					</div>
				</div>
			</div>
		</div>
	);
}
