import { Badge } from "@/components/ui/badge";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import type { ProviderCatalog } from "@dcc/contracts";
import { getProviderChips, summarizeProviderHealth } from "./provider-display";

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
								<Badge variant={summarizeProviderHealth(provider.health).variant}>
									{summarizeProviderHealth(provider.health).label}
								</Badge>
								{getProviderChips(provider)
									.filter((chip) => chip.label !== "stable" && chip.label !== "experimental" && chip.label !== summarizeProviderHealth(provider.health).label)
									.map((chip) => (
										<Badge key={`${provider.id}-${chip.label}`} variant={chip.variant}>
											{chip.label}
										</Badge>
									))}
								{provider.models.map((model) => (
									<Badge
										key={`${provider.id}-${model.id}`}
										variant={model.recommended ? "success" : "outline"}
									>
										{model.label}
									</Badge>
								))}
							</div>
						</li>
					))}
				</ul>
			</CardContent>
		</Card>
	);
}
