import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { LoaderCircle, RefreshCcw, RotateCcw } from "lucide-react";
import { toast } from "sonner";
import type { ProviderCatalog, ProviderResetCredits, ProviderUsageWindow } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { ProviderIcon } from "./provider-icons";
import {
	draftToProviderRuntimeConfig,
	getProviderRuntimeDraft,
	type ProviderRuntimeSettings,
} from "./provider-runtime-settings";
import {
	providerUsageSeverity,
	formatProviderUsageReset,
	supportsProviderAccountResets,
	supportsProviderAccountUsage,
	useProviderAccountReset,
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
	const reset = formatProviderUsageReset(window, i18n.language);

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

/** A credit the person can redeem; `id` is null when Codex only reports a count. */
type RedeemableReset = {
	id: string | null;
	title: string;
	expiresAt: Date | null;
};

function formatResetExpiry(date: Date, language: string) {
	const locale = language === "en" ? "en" : "pt-BR";
	const absolute = new Intl.DateTimeFormat(locale, {
		dateStyle: "short",
		timeStyle: "short",
	}).format(date);
	const days = Math.round((date.getTime() - Date.now()) / 86_400_000);
	const relative = new Intl.RelativeTimeFormat(locale, { numeric: "auto" }).format(
		Math.abs(days) < 1
			? Math.round((date.getTime() - Date.now()) / 3_600_000)
			: days,
		Math.abs(days) < 1 ? "hour" : "day",
	);
	return { absolute, relative };
}

export function redeemableResets(
	credits: ProviderResetCredits | null | undefined,
	fallbackTitle: (index: number) => string,
	now = Date.now(),
): RedeemableReset[] {
	if (!credits || credits.availableCount <= 0) return [];
	const listed = (credits.credits ?? [])
		.map((credit) => ({
			id: credit.id,
			title: credit.title?.trim() || "",
			expiresAt: credit.expiresAt ? new Date(credit.expiresAt) : null,
		}))
		.filter((credit) => !credit.expiresAt || credit.expiresAt.getTime() > now)
		.sort(
			(left, right) =>
				(left.expiresAt?.getTime() ?? Number.POSITIVE_INFINITY) -
				(right.expiresAt?.getTime() ?? Number.POSITIVE_INFINITY),
		);
	if (listed.length === 0) {
		return [{ id: null, title: fallbackTitle(0), expiresAt: null }];
	}
	return listed.map((credit, index) => ({
		...credit,
		title: credit.title || fallbackTitle(index + 1),
	}));
}

function ResetCreditList({
	resets,
	pendingId,
	onUse,
}: {
	resets: RedeemableReset[];
	pendingId: string | null | undefined;
	onUse: (reset: RedeemableReset) => void;
}) {
	const { t, i18n } = useTranslation("common");
	// Same-titled credits ("Full reset" twice) need a position to be told apart.
	const sameTitle = new Set(resets.map((reset) => reset.title)).size < resets.length;

	return (
		<section
			aria-label={t("settings.model.usageResetsTitle")}
			className="overflow-hidden rounded-xl border border-border/60 bg-background"
		>
			<header className="flex items-center justify-between gap-3 border-b border-border/50 px-3 py-2">
				<span className="text-[12px] font-medium">
					{t("settings.model.usageResetsTitle")}
				</span>
				<span className="text-[11px] tabular-nums text-muted-foreground">
					{t("settings.model.usageResetAvailable", { count: resets.length })}
				</span>
			</header>
			<ul className="divide-y divide-border/50">
				{resets.map((reset, index) => {
					const expiry = reset.expiresAt
						? formatResetExpiry(reset.expiresAt, i18n.language)
						: null;
					const isPending = pendingId !== undefined && pendingId === reset.id;
					const title = sameTitle
						? `${reset.title} · ${index + 1}`
						: reset.title;
					return (
						<li
							key={reset.id ?? "next"}
							className="flex items-center justify-between gap-3 px-3 py-2.5"
						>
							<div className="min-w-0">
								<p className="flex items-center gap-1.5 text-[12px] font-medium">
									<span className="truncate">{title}</span>
									{index === 0 && resets.length > 1 ? (
										<span className="shrink-0 rounded-full bg-amber-500/12 px-1.5 py-px text-[10px] font-medium text-amber-700 dark:text-amber-400">
											{t("settings.model.usageResetSoonest")}
										</span>
									) : null}
								</p>
								<p className="mt-0.5 truncate text-[11px] text-muted-foreground">
									{expiry
										? t("settings.model.usageResetExpiresRelative", {
												date: expiry.absolute,
												relative: expiry.relative,
											})
										: t("settings.model.usageResetNoExpiry")}
								</p>
							</div>
							<Button
								type="button"
								size="sm"
								variant={index === 0 ? "default" : "outline"}
								className="h-7 shrink-0 gap-1.5 px-2.5 text-[12px]"
								disabled={pendingId !== undefined}
								aria-label={t("settings.model.usageResetUseNamed", { title })}
								onClick={() => onUse({ ...reset, title })}
							>
								{isPending ? (
									<LoaderCircle className="size-3.5 animate-spin" />
								) : (
									<RotateCcw className="size-3.5" />
								)}
								{t("settings.model.usageResetAction")}
							</Button>
						</li>
					);
				})}
			</ul>
		</section>
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
		provider.capabilities,
	);
	const usageQuery = useProviderAccountUsage(provider, runtime);
	const resetMutation = useProviderAccountReset(provider.id, runtime);
	const [confirmReset, setConfirmReset] = useState<RedeemableReset | null>(null);

	useEffect(() => {
		void usageQuery.refetch();
	}, [usageQuery.refetch]);

	const usage = usageQuery.data;
	const resets = supportsProviderAccountResets(provider) && usage?.state === "available"
		? redeemableResets(usage.resetCredits, (index) =>
				index === 0
					? t("settings.model.usageResetNext")
					: t("settings.model.usageResetLabel", { index }),
			)
		: [];
	// Spending a reset while the windows still have room throws that room away.
	const lowestRemaining = usage?.state === "available" && usage.windows.length
		? Math.min(...usage.windows.map((window) => window.remainingPercent))
		: null;
	const applyReset = async (reset: RedeemableReset) => {
		try {
			const result = await resetMutation.mutateAsync(reset.id ?? undefined);
			setConfirmReset(null);
			if (result.outcome === "reset") {
				toast.success(t("settings.model.usageResetSuccess"));
			} else if (result.outcome === "noCredit") {
				toast.error(t("settings.model.usageResetNoCredit"));
			} else if (result.outcome === "nothingToReset") {
				toast.warning(t("settings.model.usageResetNothingToReset"));
			} else {
				toast.info(t("settings.model.usageResetSuccess"));
			}
			await usageQuery.refetch();
		} catch {
			toast.error(t("settings.model.usageResetError"));
		}
	};
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
				{resets.length > 0 ? (
					<ResetCreditList
						resets={resets}
						pendingId={resetMutation.isPending ? (confirmReset?.id ?? null) : undefined}
						onUse={setConfirmReset}
					/>
				) : null}
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
			<Dialog
				open={confirmReset !== null}
				onOpenChange={(open) => {
					if (!open && !resetMutation.isPending) setConfirmReset(null);
				}}
			>
				<DialogContent>
					<DialogHeader>
						<DialogTitle>{t("settings.model.usageResetConfirmTitle")}</DialogTitle>
						<DialogDescription>
							{confirmReset?.expiresAt
								? t("settings.model.usageResetConfirmBodyCredit", {
										title: confirmReset.title,
										date: formatResetExpiry(confirmReset.expiresAt, i18n.language).absolute,
									})
								: t("settings.model.usageResetConfirmBody")}
						</DialogDescription>
					</DialogHeader>
					{lowestRemaining !== null && lowestRemaining >= 10 ? (
						<p className="rounded-lg bg-amber-500/10 px-3 py-2 text-[12px] leading-relaxed text-amber-800 dark:text-amber-300">
							{t("settings.model.usageResetHeadroom", {
								percent: Math.round(lowestRemaining),
							})}
						</p>
					) : null}
					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							disabled={resetMutation.isPending}
							onClick={() => setConfirmReset(null)}
						>
							{t("settings.model.usageResetCancel")}
						</Button>
						<Button
							type="button"
							disabled={resetMutation.isPending || !confirmReset}
							onClick={() => confirmReset && void applyReset(confirmReset)}
						>
							{resetMutation.isPending ? (
								<LoaderCircle className="size-3.5 animate-spin" />
							) : null}
							{t("settings.model.usageResetConfirm")}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</Card>
	);
}

export function ProviderAccountUsagePanel({
	providers,
	runtimeSettings,
}: ProviderAccountUsagePanelProps) {
	const { t } = useTranslation("common");
	const supported = providers.filter((provider) =>
		supportsProviderAccountUsage(provider),
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
