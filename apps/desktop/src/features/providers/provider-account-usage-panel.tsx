import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { LoaderCircle, RefreshCcw } from "lucide-react";
import type { ProviderCatalog, ProviderUsageWindow } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { ProviderIcon } from "./provider-icons";
import {
	draftToProviderRuntimeConfig,
	getProviderRuntimeDraft,
	type ProviderRuntimeSettings,
} from "./provider-runtime-settings";
import {
	providerUsageSeverity,
	supportsProviderAccountUsage,
	useProviderAccountUsage,
} from "./provider-account-usage";

type ProviderAccountUsagePanelProps = {
	providers: ProviderCatalog["providers"];
	runtimeSettings: ProviderRuntimeSettings;
};

function windowLabel(
	id: string,
	durationMinutes: number | null | undefined,
	t: ReturnType<typeof useTranslation>["t"],
): string {
	const known: Record<string, string> = {
		primary: t("settings.model.usageWindowPrimary"),
		secondary: t("settings.model.usageWindowSecondary"),
		five_hour: t("settings.model.usageWindowFiveHour"),
		seven_day: t("settings.model.usageWindowSevenDay"),
		seven_day_opus: t("settings.model.usageWindowSevenDayOpus"),
		seven_day_sonnet: t("settings.model.usageWindowSevenDaySonnet"),
		overage: t("settings.model.usageWindowOverage"),
		subscription: t("settings.model.usageWindowSubscription"),
	};
	if (known[id]) return known[id];
	if (durationMinutes === 300) return t("settings.model.usageWindowFiveHour");
	if (durationMinutes === 10_080) return t("settings.model.usageWindowSevenDay");
	return id.replaceAll("_", " ");
}

function UsageWindowRow({ window }: { window: ProviderUsageWindow }) {
	const { t, i18n } = useTranslation("common");
	const severity = providerUsageSeverity(window);
	const remaining = Math.round(window.remainingPercent);
	const reset = window.resetsAt
		? new Intl.DateTimeFormat(i18n.language === "en" ? "en" : "pt-BR", {
				dateStyle: "short",
				timeStyle: "short",
			}).format(new Date(window.resetsAt))
		: null;

	return (
		<div className="space-y-1.5">
			<div className="flex items-center justify-between gap-3 text-[12px]">
				<span className="truncate text-muted-foreground">
					{windowLabel(window.id, window.windowDurationMinutes, t)}
				</span>
				<span
					className={cn(
						"shrink-0 font-medium tabular-nums",
						severity === "warning" && "text-amber-600 dark:text-amber-400",
						severity === "critical" && "text-destructive",
					)}
				>
					{t("settings.model.usageRemaining", { percent: remaining })}
				</span>
			</div>
			<div className="h-1.5 overflow-hidden rounded-full bg-muted">
				<div
					className={cn(
						"h-full rounded-full bg-foreground/45 transition-[width]",
						severity === "warning" && "bg-amber-500",
						severity === "critical" && "bg-destructive",
					)}
					style={{ width: `${Math.min(100, Math.max(0, window.usedPercent))}%` }}
				/>
			</div>
			{reset ? (
				<p className="text-[11px] text-muted-foreground/80">
					{t("settings.model.usageResetsAt", { date: reset })}
				</p>
			) : null}
		</div>
	);
}

function ProviderUsageCard({
	provider,
	runtimeSettings,
}: {
	provider: ProviderCatalog["providers"][number];
	runtimeSettings: ProviderRuntimeSettings;
}) {
	const { t, i18n } = useTranslation("common");
	const runtime = draftToProviderRuntimeConfig(
		getProviderRuntimeDraft(runtimeSettings, provider.id),
	);
	const usageQuery = useProviderAccountUsage(provider.id, runtime);

	useEffect(() => {
		void usageQuery.refetch();
	}, [usageQuery.refetch]);

	const usage = usageQuery.data;
	const updatedAt = usage?.state === "available" && usage.updatedAt
		? new Intl.DateTimeFormat(i18n.language === "en" ? "en" : "pt-BR", {
				timeStyle: "short",
			}).format(new Date(usage.updatedAt))
		: null;

	return (
		<Card className="rounded-2xl border-border/60 bg-muted/10 p-0 shadow-none">
			<CardHeader className="flex-row items-center gap-3 border-b border-border/40 px-4 py-3">
				<ProviderIcon provider={provider.id} className="size-4" />
				<div className="min-w-0 flex-1">
					<CardTitle className="text-[14px] font-medium">{provider.label}</CardTitle>
					{updatedAt ? (
						<p className="mt-0.5 text-[11px] text-muted-foreground">
							{t("settings.model.usageUpdatedAt", { time: updatedAt })}
						</p>
					) : null}
				</div>
				<Button
					type="button"
					variant="ghost"
					size="icon"
					className="size-8"
					aria-label={t("settings.model.usageRefresh")}
					disabled={usageQuery.isFetching}
					onClick={() => void usageQuery.refetch()}
				>
					{usageQuery.isFetching ? (
						<LoaderCircle className="size-3.5 animate-spin" />
					) : (
						<RefreshCcw className="size-3.5" />
					)}
				</Button>
			</CardHeader>
			<CardContent className="space-y-4 px-4 py-4">
				{usageQuery.isPending || (usageQuery.isFetching && !usage) ? (
					<p className="text-[12px] text-muted-foreground">
						{t("settings.model.usageLoading")}
					</p>
				) : usageQuery.isError ? (
					<p className="text-[12px] text-destructive">
						{t("settings.model.usageError")}
					</p>
				) : usage?.state === "awaitingActivity" ? (
					<p className="text-[12px] leading-relaxed text-muted-foreground">
						{t("settings.model.usageAwaitingActivity")}
					</p>
				) : usage?.windows.length ? (
					usage.windows.map((window) => (
						<UsageWindowRow key={window.id} window={window} />
					))
				) : (
					<p className="text-[12px] text-muted-foreground">
						{t("settings.model.usageUnavailable")}
					</p>
				)}
			</CardContent>
		</Card>
	);
}

export function ProviderAccountUsagePanel({
	providers,
	runtimeSettings,
}: ProviderAccountUsagePanelProps) {
	const { t } = useTranslation("common");
	const supported = providers.filter((provider) =>
		supportsProviderAccountUsage(provider.id),
	);
	if (supported.length === 0) return null;

	return (
		<div className="space-y-3 rounded-2xl border border-border/60 bg-background p-4">
			<div className="flex items-start justify-between gap-4">
				<div>
					<p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
						{t("settings.model.usageTitle")}
					</p>
					<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
						{t("settings.model.usageHint")}
					</p>
				</div>
				<Badge variant="outline" className="h-8 px-3 text-[12px] font-normal">
					{t("settings.model.usageBadge")}
				</Badge>
			</div>
			<div className="grid gap-3 xl:grid-cols-2">
				{supported.map((provider) => (
					<ProviderUsageCard
						key={provider.id}
						provider={provider}
						runtimeSettings={runtimeSettings}
					/>
				))}
			</div>
		</div>
	);
}
