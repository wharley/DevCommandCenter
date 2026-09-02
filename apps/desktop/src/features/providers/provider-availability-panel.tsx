import { useTranslation } from "react-i18next";
import type { ProviderCatalog } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";

type ProviderAvailabilityPanelProps = {
	providers: ProviderCatalog["providers"];
	pendingProviderIds: ReadonlySet<string>;
	errors: Readonly<Record<string, string>>;
	onChange: (providerId: string, enabled: boolean) => void;
};

export function ProviderAvailabilityPanel({
	providers,
	pendingProviderIds,
	errors,
	onChange,
}: ProviderAvailabilityPanelProps) {
	const { t } = useTranslation("common");

	return (
		<section className="space-y-3 rounded-2xl border border-border/60 bg-background p-4">
			<div>
				<h3 className="text-[13px] font-medium text-foreground">
					{t("settings.model.availabilityTitle")}
				</h3>
				<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
					{t("settings.model.availabilityHint")}
				</p>
			</div>
			<div className="space-y-2">
				{providers.map((provider) => {
					const enabled = provider.enabled ?? true;
					const pending = pendingProviderIds.has(provider.id);
					const error = errors[provider.id];
					return (
						<div
							key={provider.id}
							className={cn(
								"rounded-xl border border-border/50 bg-muted/10 px-3 py-3",
								!enabled && "bg-muted/20",
							)}
						>
							<div className="flex items-start gap-3">
								<div className="min-w-0 flex-1">
									<div className="flex flex-wrap items-center gap-2">
										<span className="text-[13px] font-medium text-foreground">
											{provider.label}
										</span>
										<Badge variant="outline" className="font-mono text-[10px]">
											{provider.id}
										</Badge>
										<Badge variant={enabled ? "success" : "outline"}>
											{enabled
												? t("settings.model.enabled")
												: t("settings.model.disabled")}
										</Badge>
									</div>
									<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
										{provider.description}
									</p>
									{error ? (
										<p
											id={`${provider.id}-availability-error`}
											className="mt-2 text-[11px] text-destructive"
											role="alert"
										>
											{error}
										</p>
									) : null}
								</div>
								<Switch
									checked={enabled}
									disabled={pending}
									onCheckedChange={(checked) => onChange(provider.id, checked)}
									aria-label={t("settings.model.availabilityToggle", {
										provider: provider.label,
									})}
									aria-describedby={error ? `${provider.id}-availability-error` : undefined}
								/>
							</div>
							{pending ? (
								<p className="mt-2 text-[11px] text-muted-foreground" aria-live="polite">
									{t("settings.model.pending")}
								</p>
							) : null}
						</div>
					);
				})}
			</div>
		</section>
	);
}
