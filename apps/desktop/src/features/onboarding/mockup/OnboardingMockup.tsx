import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";

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

export function OnboardingMockup() {
	return (
		<div className="relative h-full w-full overflow-hidden rounded-[28px] border border-border/60 bg-[radial-gradient(circle_at_20%_0%,color-mix(in_oklch,var(--workspace-pr-open-accent)_20%,transparent),transparent_28%),linear-gradient(180deg,color-mix(in_oklch,var(--background)_92%,transparent),color-mix(in_oklch,var(--sidebar)_96%,transparent))] p-4">
			<div className="grid h-full grid-cols-[160px_minmax(0,1fr)_272px] gap-3">
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
								Compose, steer, and review in one place.
							</p>
						</div>
						<Badge variant="outline">Cmd/Ctrl+K</Badge>
					</div>
					<div className="flex min-h-0 flex-1 flex-col justify-between rounded-2xl border border-border/60 bg-sidebar/70 p-3">
						<div className="space-y-2">
							<div className="h-3 w-36 rounded-full bg-muted/60" />
							<div className="h-3 w-52 rounded-full bg-muted/40" />
							<div className="h-3 w-44 rounded-full bg-muted/50" />
						</div>
						<div className="rounded-xl border border-border/60 bg-background/85 p-3">
							<p className="text-[12px] text-muted-foreground">
								The composer and thread viewport stay in lockstep with the selected workspace.
							</p>
						</div>
					</div>
				</div>

				<div className="flex flex-col gap-3 rounded-2xl border border-border/60 bg-sidebar/80 p-3">
					<MockWindow title="Inspector" />
					<MockWindow title="Session state" />
					<MockWindow title="Branches" />
				</div>
			</div>
		</div>
	);
}
