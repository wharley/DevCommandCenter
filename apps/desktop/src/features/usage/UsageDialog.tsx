import { useQuery } from "@tanstack/react-query";
import type {
	DailyUsageSummary,
	ProviderCatalog,
	ProviderUsageSummary,
	UsageDashboard,
} from "@dcc/contracts";
import {
	Activity,
	BarChart3,
	Coins,
	Gauge,
	Loader2,
	RefreshCw,
	Sparkles,
} from "lucide-react";
import { memo, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { loadUsageDashboard } from "@/lib/usage-api";
import { cn } from "@/lib/utils";

type UsageDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	providerCatalog: ProviderCatalog;
	projectId?: string | null;
};

type Period = 7 | 30 | 90 | null;

type UsageUnitLabels = {
	tokens: string;
	turns: string;
};

const PROVIDER_COLORS: Record<string, string> = {
	codex: "#34d399",
	claude_code: "#fb923c",
	gemini: "#60a5fa",
	cursor: "#a78bfa",
	droid: "#22d3ee",
	grok: "#f472b6",
};

export function providerUsageColor(providerId: string) {
	return PROVIDER_COLORS[providerId] ?? "#94a3b8";
}

function compactNumber(value: number) {
	return new Intl.NumberFormat(undefined, {
		notation: value >= 10_000 ? "compact" : "standard",
		maximumFractionDigits: value >= 10_000 ? 1 : 0,
	}).format(value);
}

function percent(value: number, total: number) {
	if (total <= 0) return 0;
	return Math.min(100, Math.max(0, (value / total) * 100));
}

function providerMetric(provider: ProviderUsageSummary, useTokens: boolean) {
	return useTokens ? provider.totalTokens : provider.turns;
}

const ProviderDonut = memo(function ProviderDonut({
	data,
	labels,
	useTokens,
	units,
}: {
	data: ProviderUsageSummary[];
	labels: Map<string, string>;
	useTokens: boolean;
	units: UsageUnitLabels;
}) {
	const total = data.reduce(
		(sum, provider) => sum + providerMetric(provider, useTokens),
		0,
	);
	let cursor = 0;
	const stops = data
		.filter((provider) => providerMetric(provider, useTokens) > 0)
		.map((provider) => {
			const start = cursor;
			cursor += percent(providerMetric(provider, useTokens), total);
			return `${providerUsageColor(provider.providerId)} ${start}% ${cursor}%`;
		});

	return (
		<div className="grid min-w-0 gap-5 sm:grid-cols-[180px_minmax(0,1fr)] sm:items-center">
			<div className="relative mx-auto size-40">
				<div
					className="size-full rounded-full"
					style={{
						background:
							stops.length > 0
								? `conic-gradient(${stops.join(", ")})`
								: "var(--muted)",
					}}
				/>
				<div className="absolute inset-5 flex flex-col items-center justify-center rounded-full bg-popover ring-1 ring-border/50">
					<strong className="text-xl font-semibold tabular-nums">
						{compactNumber(total)}
					</strong>
					<span className="text-[10px] uppercase tracking-[0.08em] text-muted-foreground">
						{useTokens ? units.tokens : units.turns}
					</span>
				</div>
			</div>
			<div className="grid min-w-0 gap-2">
				{data.map((provider, index) => {
					const value = providerMetric(provider, useTokens);
					const share = percent(value, total);
					return (
						<div
							key={provider.providerId}
							className="grid min-w-0 grid-cols-[20px_minmax(0,1fr)_auto] items-center gap-2 rounded-md px-2 py-1.5 hover:bg-muted/40"
						>
							<span className="text-center text-[11px] text-muted-foreground">
								{index + 1}
							</span>
							<div className="min-w-0">
								<div className="flex min-w-0 items-center gap-2">
									<span
										className="size-2 shrink-0 rounded-full"
										style={{ backgroundColor: providerUsageColor(provider.providerId) }}
									/>
									<span className="truncate text-[12px] font-medium">
										{labels.get(provider.providerId) ?? provider.providerId}
									</span>
								</div>
								<div className="mt-1 h-1 overflow-hidden rounded-full bg-muted">
									<div
										className="h-full rounded-full"
										style={{
											backgroundColor: providerUsageColor(provider.providerId),
											width: `${share}%`,
										}}
									/>
								</div>
							</div>
							<strong className="text-[12px] tabular-nums">{share.toFixed(1)}%</strong>
						</div>
					);
				})}
			</div>
		</div>
	);
});

const UsageTimeline = memo(function UsageTimeline({
	daily,
	providers,
	useTokens,
	ariaLabel,
}: {
	daily: DailyUsageSummary[];
	providers: ProviderUsageSummary[];
	useTokens: boolean;
	ariaLabel: string;
}) {
	const dates = [...new Set(daily.map((item) => item.date))].sort();
	const width = 760;
	const height = 180;
	const padX = 14;
	const padY = 14;
	const values = daily.map((item) => (useTokens ? item.totalTokens : item.turns));
	const maximum = Math.max(1, ...values);
	const paths = providers.map((provider) => {
		const byDate = new Map(
			daily
				.filter((item) => item.providerId === provider.providerId)
				.map((item) => [item.date, useTokens ? item.totalTokens : item.turns]),
		);
		const points = dates.map((date, index) => {
			const x =
				dates.length <= 1
					? width / 2
					: padX + (index / (dates.length - 1)) * (width - padX * 2);
			const value = byDate.get(date) ?? 0;
			const y = height - padY - (value / maximum) * (height - padY * 2);
			return `${index === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
		});
		return { providerId: provider.providerId, path: points.join(" ") };
	});

	if (dates.length === 0) {
		return <div className="flex h-44 items-center justify-center text-xs text-muted-foreground">—</div>;
	}

	return (
		<div className="min-w-0">
			<svg
				viewBox={`0 0 ${width} ${height}`}
				className="h-44 w-full overflow-visible"
				role="img"
				aria-label={ariaLabel}
				preserveAspectRatio="none"
			>
				{[0.25, 0.5, 0.75].map((ratio) => (
					<line
						key={ratio}
						x1={padX}
						x2={width - padX}
						y1={height * ratio}
						y2={height * ratio}
						stroke="currentColor"
						className="text-border/50"
						strokeDasharray="3 5"
						vectorEffect="non-scaling-stroke"
					/>
				))}
				{paths.map(({ providerId, path }) => (
					<path
						key={providerId}
						d={path}
						fill="none"
						stroke={providerUsageColor(providerId)}
						strokeWidth="2"
						strokeLinecap="round"
						strokeLinejoin="round"
						vectorEffect="non-scaling-stroke"
					/>
				))}
			</svg>
			<div className="mt-1 flex justify-between text-[10px] text-muted-foreground">
				<span>{new Date(`${dates[0]}T00:00:00Z`).toLocaleDateString()}</span>
				<span>{new Date(`${dates.at(-1)}T00:00:00Z`).toLocaleDateString()}</span>
			</div>
		</div>
	);
});

function SummaryCard({
	icon: Icon,
	label,
	value,
	hint,
}: {
	icon: typeof Activity;
	label: string;
	value: string;
	hint: string;
}) {
	return (
		<div className="min-w-0 rounded-lg border border-border/60 bg-muted/15 p-3">
			<div className="flex items-center gap-2 text-muted-foreground">
				<Icon className="size-3.5" aria-hidden />
				<span className="truncate text-[10px] font-semibold uppercase tracking-[0.07em]">
					{label}
				</span>
			</div>
			<strong className="mt-2 block truncate text-xl font-semibold tabular-nums">{value}</strong>
			<p className="mt-0.5 truncate text-[10px] text-muted-foreground">{hint}</p>
		</div>
	);
}

function UsageContent({
	dashboard,
	providerCatalog,
}: {
	dashboard: UsageDashboard;
	providerCatalog: ProviderCatalog;
}) {
	const { t } = useTranslation("common");
	const labels = useMemo(
		() => new Map(providerCatalog.providers.map((provider) => [provider.id, provider.label])),
		[providerCatalog.providers],
	);
	const useTokens = dashboard.totals.totalTokens > 0;
	const coverage = percent(dashboard.totals.measuredTurns, dashboard.totals.turns);
	const topModels = dashboard.models.slice(0, 8);
	const units = {
		tokens: t("usage.units.tokens"),
		turns: t("usage.units.turns"),
	};

	return (
		<div className="grid min-w-0 gap-4">
			<div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
				<SummaryCard
					icon={Sparkles}
					label={t("usage.metrics.tokens")}
					value={compactNumber(dashboard.totals.totalTokens)}
					hint={t("usage.metrics.tokenBreakdown", {
						input: compactNumber(dashboard.totals.inputTokens),
						output: compactNumber(dashboard.totals.outputTokens),
					})}
				/>
				<SummaryCard
					icon={Activity}
					label={t("usage.metrics.activity")}
					value={compactNumber(dashboard.totals.turns)}
					hint={t("usage.metrics.sessions", { count: dashboard.totals.sessions })}
				/>
				<SummaryCard
					icon={Gauge}
					label={t("usage.metrics.coverage")}
					value={`${coverage.toFixed(0)}%`}
					hint={t("usage.metrics.measuredTurns", {
						count: dashboard.totals.measuredTurns,
					})}
				/>
				<SummaryCard
					icon={Coins}
					label={t("usage.metrics.cost")}
					value={
						dashboard.totals.costUsd == null
							? "—"
							: new Intl.NumberFormat(undefined, {
									style: "currency",
									currency: "USD",
									maximumFractionDigits: 2,
								}).format(dashboard.totals.costUsd)
					}
					hint={t("usage.metrics.reportedCost")}
				/>
			</div>

			<div className="grid min-w-0 gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.15fr)]">
				<section className="min-w-0 rounded-lg border border-border/60 p-4">
					<div className="mb-3 flex items-center justify-between gap-2">
						<div>
							<h3 className="text-[13px] font-medium">{t("usage.arena.title")}</h3>
							<p className="text-[10px] text-muted-foreground">
								{useTokens ? t("usage.arena.byTokens") : t("usage.arena.byTurns")}
							</p>
						</div>
						<Badge variant="secondary" className="text-[10px]">
							{t("usage.arena.providerCount", { count: dashboard.providers.length })}
						</Badge>
					</div>
					<ProviderDonut
						data={dashboard.providers}
						labels={labels}
						useTokens={useTokens}
						units={units}
					/>
				</section>

				<section className="min-w-0 rounded-lg border border-border/60 p-4">
					<div className="mb-2">
						<h3 className="text-[13px] font-medium">{t("usage.timeline.title")}</h3>
						<p className="text-[10px] text-muted-foreground">
							{useTokens ? t("usage.timeline.tokens") : t("usage.timeline.turns")}
						</p>
					</div>
					<UsageTimeline
						daily={dashboard.daily}
						providers={dashboard.providers}
						useTokens={useTokens}
						ariaLabel={t("usage.timeline.ariaLabel")}
					/>
				</section>
			</div>

			<div className="grid min-w-0 gap-4 lg:grid-cols-2">
				<section className="min-w-0 rounded-lg border border-border/60 p-4">
					<h3 className="text-[13px] font-medium">{t("usage.providers.title")}</h3>
					<div className="mt-3 grid gap-2 sm:grid-cols-2">
						{dashboard.providers.map((provider) => (
							<div key={provider.providerId} className="min-w-0 rounded-md bg-muted/30 p-3">
								<div className="flex min-w-0 items-center gap-2">
									<span
										className="size-2.5 shrink-0 rounded-full"
										style={{ backgroundColor: providerUsageColor(provider.providerId) }}
									/>
									<strong className="truncate text-[12px]">
										{labels.get(provider.providerId) ?? provider.providerId}
									</strong>
									{provider.measuredTurns === 0 ? (
										<Badge variant="outline" className="ml-auto text-[9px]">
											{t("usage.providers.noTelemetry")}
										</Badge>
									) : null}
								</div>
								<div className="mt-3 grid grid-cols-3 gap-2 text-center">
									<div>
										<strong className="block text-[13px] tabular-nums">{provider.sessions}</strong>
										<span className="text-[9px] text-muted-foreground">{t("usage.units.sessions")}</span>
									</div>
									<div>
										<strong className="block text-[13px] tabular-nums">{provider.turns}</strong>
										<span className="text-[9px] text-muted-foreground">{units.turns}</span>
									</div>
									<div>
										<strong className="block text-[13px] tabular-nums">
											{compactNumber(provider.totalTokens)}
										</strong>
										<span className="text-[9px] text-muted-foreground">{units.tokens}</span>
									</div>
								</div>
							</div>
						))}
					</div>
				</section>

				<section className="min-w-0 rounded-lg border border-border/60 p-4">
					<h3 className="text-[13px] font-medium">{t("usage.models.title")}</h3>
					{topModels.length === 0 ? (
						<p className="py-10 text-center text-xs text-muted-foreground">
							{t("usage.models.awaiting")}
						</p>
					) : (
						<div className="mt-3 grid gap-2">
							{topModels.map((model, index) => (
								<div
									key={`${model.providerId}:${model.model}`}
									className="grid min-w-0 grid-cols-[20px_minmax(0,1fr)_auto] items-center gap-2 rounded-md bg-muted/25 px-2.5 py-2"
								>
									<span className="text-center text-[10px] text-muted-foreground">{index + 1}</span>
									<div className="min-w-0">
										<p className="truncate text-[11px] font-medium">{model.model}</p>
										<p className="truncate text-[9px] text-muted-foreground">
											{labels.get(model.providerId) ?? model.providerId} · {t("usage.models.turnCount", { count: model.measuredTurns })}
										</p>
									</div>
									<strong className="text-[11px] tabular-nums">{compactNumber(model.totalTokens)}</strong>
								</div>
							))}
						</div>
					)}
				</section>
			</div>
		</div>
	);
}

export function UsageDialog({
	open,
	onOpenChange,
	providerCatalog,
	projectId = null,
}: UsageDialogProps) {
	const { t } = useTranslation("common");
	const [period, setPeriod] = useState<Period>(30);
	const [projectOnly, setProjectOnly] = useState(false);
	const query = useQuery({
		queryKey: ["usage", "dashboard", period ?? "all", projectOnly ? projectId : null],
		queryFn: () =>
			loadUsageDashboard({
				periodDays: period,
				projectId: projectOnly ? projectId : null,
			}),
		enabled: open,
		staleTime: 30_000,
		gcTime: 300_000,
	});

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-h-[calc(100dvh-2rem)] min-w-0 w-full overflow-hidden p-0 sm:max-w-6xl">
				<DialogHeader className="min-w-0 border-b border-border/60 px-5 pb-4 pt-5 pr-14">
					<div className="flex min-w-0 flex-wrap items-start justify-between gap-3">
						<div className="min-w-0">
							<DialogTitle className="flex items-center gap-2">
								<BarChart3 className="size-4 text-emerald-400" aria-hidden />
								{t("usage.title")}
							</DialogTitle>
							<DialogDescription className="mt-1">{t("usage.description")}</DialogDescription>
						</div>
						<div className="flex flex-wrap items-center justify-end gap-1.5">
							{([7, 30, 90, null] as const).map((value) => (
								<Button
									key={value ?? "all"}
									type="button"
									variant={period === value ? "secondary" : "ghost"}
									size="xs"
									onClick={() => setPeriod(value)}
								>
									{value == null ? t("usage.period.all") : t("usage.period.days", { count: value })}
								</Button>
							))}
							{projectId ? (
								<Button
									type="button"
									variant={projectOnly ? "secondary" : "outline"}
									size="xs"
									onClick={() => setProjectOnly((value) => !value)}
								>
									{t("usage.period.project")}
								</Button>
							) : null}
							<Button
								type="button"
								variant="ghost"
								size="icon-xs"
								aria-label={t("usage.refresh")}
								disabled={query.isFetching}
								onClick={() => void query.refetch()}
							>
								<RefreshCw className={cn("size-3.5", query.isFetching && "animate-spin")} />
							</Button>
						</div>
					</div>
				</DialogHeader>

				<ScrollArea className="min-h-0 flex-1" viewportProps={{ className: "max-h-[calc(100dvh-9rem)]" }}>
					<div className="min-w-0 p-5">
						{query.isPending ? (
							<div className="flex min-h-80 items-center justify-center gap-2 text-sm text-muted-foreground">
								<Loader2 className="size-4 animate-spin" />
								{t("usage.loading")}
							</div>
						) : query.isError ? (
							<div className="flex min-h-80 flex-col items-center justify-center gap-3 text-center">
								<p className="text-sm text-destructive">{t("usage.error")}</p>
								<Button variant="outline" size="sm" onClick={() => void query.refetch()}>
									{t("usage.tryAgain")}
								</Button>
							</div>
						) : query.data && query.data.providers.length > 0 ? (
							<UsageContent dashboard={query.data} providerCatalog={providerCatalog} />
						) : (
							<div className="flex min-h-80 flex-col items-center justify-center gap-2 text-center">
								<BarChart3 className="size-8 text-muted-foreground/50" />
								<p className="text-sm font-medium">{t("usage.empty.title")}</p>
								<p className="max-w-sm text-xs text-muted-foreground">{t("usage.empty.body")}</p>
							</div>
						)}
					</div>
				</ScrollArea>
			</DialogContent>
		</Dialog>
	);
}
