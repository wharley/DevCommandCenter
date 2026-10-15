import { useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useIsFetching, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, ChevronRight } from "lucide-react";
import type { ProviderCatalog } from "@dcc/contracts";
import { Switch } from "@/components/ui/switch";
import { listProviderAccounts } from "@/lib/provider-api";
import { cn } from "@/lib/utils";
import { ProviderAccountsList } from "./provider-accounts-section";
import { ProviderUsageDetails } from "./provider-account-usage-panel";
import { supportsProviderAccountUsage } from "./provider-account-usage";
import { accountsOfProvider, PROVIDER_ACCOUNTS_QUERY_KEY } from "./provider-accounts";
import { ProviderIcon } from "./provider-icons";
import { AntigravitySetup, ProviderRuntimeFields } from "./provider-runtime-panel";
import {
	draftToProviderRuntimeConfig,
	getProviderRuntimeDraft,
	supportsProviderRuntime,
	type ProviderRuntimeDraft,
	type ProviderRuntimeSettings,
} from "./provider-runtime-settings";
import { localizedProviderSetupReason } from "./provider-setup-message";
import { modelDescriptionText, providerDescriptionText } from "./catalog-text";
import { providerRailStatus, type ProviderRailStatus } from "./providers-settings.logic";

/** Signing in from a terminal does not tell DCC; this reruns the health checks. */
const PROVIDER_CATALOG_QUERY_KEY = ["providers", "catalog"] as const;

type Provider = ProviderCatalog["providers"][number];

export type ProvidersSettingsProps = {
	providers: ProviderCatalog["providers"];
	pendingAvailabilityIds: ReadonlySet<string>;
	availabilityErrors: Readonly<Record<string, string>>;
	onChangeAvailability: (providerId: string, enabled: boolean) => void;
	runtimeSettings: ProviderRuntimeSettings;
	onChangeRuntime: (providerId: string, draft: ProviderRuntimeDraft) => void;
	onClearRuntime: (providerId: string) => void;
	selectedProviderId: string | null;
	selectedModelId: string | null;
	onSelectProvider: (providerId: string) => void;
	onSelectModel: (modelId: string) => void;
};

const STATUS_DOT: Record<ProviderRailStatus["tone"], string> = {
	ready: "bg-emerald-500",
	attention: "bg-amber-500",
	off: "bg-muted-foreground/35",
};

function DetailSection({
	title,
	hint,
	children,
}: {
	title: string;
	hint?: string;
	children: ReactNode;
}) {
	return (
		<section className="space-y-3 border-t border-border/50 px-6 py-5">
			<div>
				<h4 className="text-[13px] font-medium text-foreground">{title}</h4>
				{hint ? (
					<p className="mt-0.5 max-w-[60ch] text-[12px] leading-relaxed text-muted-foreground">
						{hint}
					</p>
				) : null}
			</div>
			{children}
		</section>
	);
}

function ModelList({
	provider,
	selectedModelId,
	isSelectedProvider,
	onPick,
}: {
	provider: Provider;
	selectedModelId: string | null;
	isSelectedProvider: boolean;
	onPick: (modelId: string) => void;
}) {
	const { t } = useTranslation("common");
	const enabled = provider.enabled !== false;
	if (provider.models.length === 0) {
		return <p className="text-[12px] text-muted-foreground">{t("settings.model.modelsEmpty")}</p>;
	}
	return (
		<ul className="divide-y divide-border/50 overflow-hidden rounded-lg border border-border/60 bg-background">
			{provider.models.map((model) => {
				const selected = isSelectedProvider && model.id === selectedModelId;
				return (
					<li key={model.id}>
						<button
							type="button"
							disabled={!enabled}
							onClick={() => onPick(model.id)}
							aria-pressed={selected}
							className={cn(
								"flex w-full items-center gap-3 px-3 py-2 text-left transition-colors hover:bg-accent/40 focus-visible:bg-accent/50 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:bg-transparent",
								selected && "bg-accent/50",
							)}
						>
							<span className="min-w-0 flex-1">
								<span className="flex items-center gap-2">
									<span className="truncate text-[13px] text-foreground">{model.label}</span>
									{model.recommended ? (
										<span className="shrink-0 text-[11px] text-muted-foreground">
											{t("settings.model.modelRecommended")}
										</span>
									) : null}
								</span>
								{model.description ? (
									<span className="mt-0.5 block truncate text-[12px] text-muted-foreground">
										{modelDescriptionText(model.description, t)}
									</span>
								) : null}
							</span>
							{selected ? <Check className="size-4 shrink-0 text-foreground" aria-hidden /> : null}
						</button>
					</li>
				);
			})}
		</ul>
	);
}

function ProviderDetail({
	provider,
	props,
}: {
	provider: Provider;
	props: ProvidersSettingsProps;
}) {
	const { t } = useTranslation("common");
	const draft = getProviderRuntimeDraft(props.runtimeSettings, provider.id);
	const baseRuntime = draftToProviderRuntimeConfig(draft, provider.capabilities);
	const enabled = provider.enabled !== false;
	const pending = props.pendingAvailabilityIds.has(provider.id);
	const availabilityError = props.availabilityErrors[provider.id];
	const health = provider.health;
	const healthProblem =
		health && typeof health === "object"
			? "Unhealthy" in health
				? { reason: health.Unhealthy?.reason }
				: "Degraded" in health
					? { reason: health.Degraded?.reason }
					: null
			: null;
	const customizedRuntime = Boolean(
		draft.binaryPath.trim() ||
			draft.homePath.trim() ||
			draft.shadowHomePath.trim() ||
			draft.maxConcurrentSubagents,
	);
	const [advancedOpen, setAdvancedOpen] = useState(customizedRuntime);
	const queryClient = useQueryClient();
	const checking = useIsFetching({ queryKey: PROVIDER_CATALOG_QUERY_KEY }) > 0;

	return (
		<div className="min-w-0">
			<header className="flex items-start gap-3 px-6 pb-5 pt-6">
				<ProviderIcon provider={provider.id} className="mt-0.5 size-5 shrink-0" />
				<div className="min-w-0 flex-1">
					<div className="flex items-baseline gap-2">
						<h3 className="truncate text-[16px] font-semibold text-foreground">{provider.label}</h3>
						{provider.stable ? null : (
							<span className="shrink-0 text-[11px] text-muted-foreground">
								{t("settings.model.experimental")}
							</span>
						)}
					</div>
					<p className="mt-1 max-w-[60ch] text-[12px] leading-relaxed text-muted-foreground">
						{providerDescriptionText(provider, t)}
					</p>
					{enabled && healthProblem?.reason ? (
						<p className="mt-2 max-w-[60ch] text-[12px] leading-relaxed text-amber-600 dark:text-amber-400">
							{localizedProviderSetupReason(healthProblem.reason, t) ?? healthProblem.reason}{" "}
							<button
								type="button"
								disabled={checking}
								onClick={() =>
									void queryClient.invalidateQueries({ queryKey: PROVIDER_CATALOG_QUERY_KEY })
								}
								className="font-medium text-foreground underline-offset-2 hover:underline disabled:text-muted-foreground disabled:no-underline"
							>
								{checking ? t("settings.model.rechecking") : t("settings.model.recheck")}
							</button>
						</p>
					) : null}
					{!enabled ? (
						<p className="mt-2 max-w-[60ch] text-[12px] leading-relaxed text-muted-foreground">
							{t("settings.model.availabilityHint")}
						</p>
					) : null}
					{availabilityError ? (
						<p className="mt-2 text-[12px] text-destructive" role="alert">
							{availabilityError}
						</p>
					) : null}
				</div>
				<label className="flex shrink-0 items-center gap-2 pt-0.5 text-[12px] text-muted-foreground">
					{pending
						? t("settings.model.pending")
						: enabled
							? t("settings.model.enabled")
							: t("settings.model.disabled")}
					<Switch
						checked={enabled}
						disabled={pending}
						onCheckedChange={(checked) => props.onChangeAvailability(provider.id, checked)}
						aria-label={t("settings.model.availabilityToggle", { provider: provider.label })}
					/>
				</label>
			</header>

			{provider.id === "antigravity" ? (
				<DetailSection title={t("settings.model.antigravitySetupTitle")}>
					<AntigravitySetup provider={provider} draft={draft} />
				</DetailSection>
			) : null}

			{provider.capabilities.supportsAccounts ? (
				<DetailSection
					title={t("settings.model.accountsTitle")}
					hint={t("settings.model.accountsHint")}
				>
					<ProviderAccountsList provider={provider} baseRuntime={baseRuntime} />
				</DetailSection>
			) : supportsProviderAccountUsage(provider) ? (
				<DetailSection title={t("settings.model.usageTitle")}>
					<ProviderUsageDetails provider={provider} runtime={baseRuntime} />
				</DetailSection>
			) : null}

			<DetailSection
				title={t("settings.model.modelsTitle")}
				hint={t("settings.model.modelsHint")}
			>
				<ModelList
					provider={provider}
					selectedModelId={props.selectedModelId}
					isSelectedProvider={props.selectedProviderId === provider.id}
					onPick={(modelId) => {
						if (props.selectedProviderId !== provider.id) props.onSelectProvider(provider.id);
						props.onSelectModel(modelId);
					}}
				/>
			</DetailSection>

			{supportsProviderRuntime(provider.capabilities) ? (
				<section className="border-t border-border/50 px-6 py-4">
					<button
						type="button"
						aria-expanded={advancedOpen}
						onClick={() => setAdvancedOpen((open) => !open)}
						className="flex w-full items-center gap-2 text-left focus-visible:outline-none"
					>
						<ChevronRight
							className={cn(
								"size-3.5 text-muted-foreground transition-transform",
								advancedOpen && "rotate-90",
							)}
							aria-hidden
						/>
						<span className="text-[13px] font-medium text-foreground">
							{t("settings.model.advancedTitle")}
						</span>
						<span className="text-[12px] text-muted-foreground">
							{customizedRuntime
								? t("settings.model.advancedCustomized")
								: t("settings.model.advancedHint")}
						</span>
					</button>
					{advancedOpen ? (
						<div className="mt-4 max-w-lg pl-5">
							<ProviderRuntimeFields
								provider={provider}
								draft={draft}
								onChangeRuntime={props.onChangeRuntime}
								onClearRuntime={props.onClearRuntime}
							/>
						</div>
					) : null}
				</section>
			) : null}
		</div>
	);
}

/**
 * Settings → Providers: the providers on the left with one line of status,
 * and everything about the chosen one on the right — whether it is on, its
 * accounts with how much of each plan is left, the model new tasks start
 * with, and how DCC launches it.
 */
export function ProvidersSettings(props: ProvidersSettingsProps) {
	const { t } = useTranslation("common");
	const accountsQuery = useQuery({
		queryKey: PROVIDER_ACCOUNTS_QUERY_KEY,
		queryFn: listProviderAccounts,
		staleTime: 30_000,
	});
	const [chosenId, setChosenId] = useState<string | null>(null);
	const providers = props.providers;
	const current =
		providers.find((provider) => provider.id === chosenId) ??
		providers.find((provider) => provider.id === props.selectedProviderId) ??
		providers[0] ??
		null;
	const statuses = useMemo(
		() =>
			new Map(
				providers.map((provider) => [
					provider.id,
					providerRailStatus(
						provider,
						accountsOfProvider(accountsQuery.data ?? [], provider.id).length,
					),
				]),
			),
		[accountsQuery.data, providers],
	);

	if (!current) {
		return (
			<p className="rounded-xl border border-dashed border-border/60 p-6 text-[12px] text-muted-foreground">
				{t("settings.model.runtimeNoSupportedProviders")}
			</p>
		);
	}

	// Sized by the settings pane, not the window: below ~640px of pane the
	// provider list becomes a scrolling strip above the detail.
	return (
		<div className="@container">
		<div className="grid min-h-[520px] grid-cols-1 overflow-hidden rounded-xl border border-border/60 bg-background @2xl:grid-cols-[216px_minmax(0,1fr)]">
			<nav
				aria-label={t("settings.model.providersNav")}
				className="border-b border-border/60 bg-muted/25 p-2 @2xl:border-b-0 @2xl:border-r"
			>
				<ul className="flex gap-1 overflow-x-auto @2xl:flex-col @2xl:overflow-visible">
					{providers.map((provider) => {
						const status = statuses.get(provider.id);
						const active = provider.id === current.id;
						return (
							<li key={provider.id} className="shrink-0">
								<button
									type="button"
									aria-current={active ? "page" : undefined}
									onClick={() => setChosenId(provider.id)}
									className={cn(
										"flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
										active
											? "bg-background shadow-sm ring-1 ring-border/60"
											: "hover:bg-background/60",
									)}
								>
									<ProviderIcon
										provider={provider.id}
										className={cn("size-4 shrink-0", provider.enabled === false && "opacity-45")}
									/>
									<span className="min-w-0 flex-1">
										<span
											className={cn(
												"block truncate text-[13px]",
												active ? "font-medium text-foreground" : "text-foreground/85",
											)}
										>
											{provider.label}
										</span>
										{status ? (
											<span className="mt-0.5 flex items-center gap-1.5 text-[11px] text-muted-foreground">
												<span
													className={cn("size-1.5 shrink-0 rounded-full", STATUS_DOT[status.tone])}
													aria-hidden
												/>
												<span className="truncate">
													{t(`settings.model.${status.key}`, { count: status.count })}
												</span>
											</span>
										) : null}
									</span>
								</button>
							</li>
						);
					})}
				</ul>
			</nav>
			<ProviderDetail key={current.id} provider={current} props={props} />
		</div>
		</div>
	);
}
