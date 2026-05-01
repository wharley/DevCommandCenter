import { Badge } from "@/components/ui/badge";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import type { ProviderCatalog } from "@dcc/contracts";

function healthSummary(health: ProviderCatalog["providers"][number]["health"]) {
	if (health === "Healthy") {
		return { label: "healthy", variant: "success" as const };
	}
	if (health && typeof health === "object") {
		if ("Degraded" in health) {
			return {
				label: health.Degraded?.reason ?? "degraded",
				variant: "outline" as const,
			};
		}
		if ("Unhealthy" in health) {
			return {
				label: health.Unhealthy?.reason ?? "unhealthy",
				variant: "outline" as const,
			};
		}
	}
	return { label: "unknown", variant: "outline" as const };
}

export function ProviderCatalogCard({
	catalog,
}: {
	catalog: ProviderCatalog | null;
}) {
	const providers = catalog?.providers ?? [];

	if (providers.length === 0) {
		return (
			<Card className="dcc-runtime-feed border-[var(--dcc-shell-border)]">
				<CardHeader className="pb-2">
					<CardTitle className="text-sm font-medium">Providers</CardTitle>
					<CardDescription className="text-[12px] text-[var(--dcc-text-muted)]">
						None registered yet. When the Rust runtime exposes CLIs, they list here.
					</CardDescription>
				</CardHeader>
			</Card>
		);
	}

	const stableCount = providers.filter((provider) => provider.stable).length;

	return (
		<Card className="dcc-runtime-feed border-[var(--dcc-shell-border)]">
			<CardHeader className="pb-2">
				<div className="dcc-card__meta-row">
					<CardTitle className="text-sm font-medium">Providers</CardTitle>
					<Badge variant="outline" className="font-normal">
						{stableCount}/{providers.length} stable
					</Badge>
				</div>
			</CardHeader>
			<CardContent className="dcc-runtime-feed__content pt-0">
				<ul className="dcc-runtime-feed__list">
					{providers.map((provider) => (
						<li key={provider.id}>
							<div className="dcc-runtime-feed__row">
								<strong>{provider.label}</strong>
								<small>{provider.description}</small>
							</div>
							<div className="dcc-runtime-feed__chips">
								<Badge variant={provider.stable ? "success" : "outline"}>
									{provider.stable ? "stable" : "experimental"}
								</Badge>
								<Badge variant={healthSummary(provider.health).variant}>
									{healthSummary(provider.health).label}
								</Badge>
								{provider.capabilities.streaming ? (
									<Badge variant="outline">streaming</Badge>
								) : null}
								{provider.capabilities.tools ? (
									<Badge variant="outline">tools</Badge>
								) : null}
								{provider.capabilities.mcp ? (
									<Badge variant="outline">mcp</Badge>
								) : null}
								{provider.capabilities.resumable ? (
									<Badge variant="outline">resumable</Badge>
								) : null}
							</div>
						</li>
					))}
				</ul>
			</CardContent>
		</Card>
	);
}
