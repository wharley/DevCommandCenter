import { useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import { LoaderCircle, RefreshCcw } from "lucide-react";
import type { ProviderAccountUsage, ProviderCatalog } from "@dcc/contracts";
import { cn } from "@/lib/utils";
import {
	formatProviderUsageReset,
	providerUsageSeverity,
} from "@/features/providers/provider-account-usage";

export const DCC_OPEN_MODEL_PICKER_EVENT = "dcc:open-model-picker";

export type ComposerExecutionMenuProps = {
	compact?: boolean;
	onReturnToComposer?: () => void;
	open: boolean;
	onOpenChange: (open: boolean) => void;
	providers: ProviderCatalog["providers"];
	selectedProviderId: string | null;
	selectedModelId: string | null;
	availableEffortLevels: readonly string[];
	selectedEffortId: string;
	directResponse: boolean;
	onSelectProvider: (providerId: string) => void;
	onSelectModel: (modelId: string) => void;
	onSelectEffort: (effortId: string) => void;
	onSelectUltrathink: () => void;
	onSetDirectResponse: (direct: boolean) => void;
	accountUsage?: ProviderAccountUsage | null;
	isAccountUsageFetching?: boolean;
	hasAccountUsageError?: boolean;
	onRefreshAccountUsage?: () => void;
	disabled?: boolean;
};

/** Resolves the provider/model pair the trigger shows, tolerating a stale provider id. */
export function useComposerSelection(
	providers: ProviderCatalog["providers"],
	selectedProviderId: string | null,
	selectedModelId: string | null,
) {
	const selectedProvider = useMemo(() => {
		const explicit =
			providers.find((provider) => provider.id === selectedProviderId) ?? null;
		if (
			explicit &&
			(!selectedModelId ||
				explicit.models.some((model) => model.id === selectedModelId))
		) {
			return explicit;
		}
		if (selectedModelId) {
			const owner = providers.find((provider) =>
				provider.models.some((model) => model.id === selectedModelId),
			);
			if (owner) return owner;
		}
		return explicit ?? providers[0] ?? null;
	}, [providers, selectedProviderId, selectedModelId]);
	const selectedModel = useMemo(() => {
		if (!selectedModelId || !selectedProvider) return null;
		return (
			selectedProvider.models.find((model) => model.id === selectedModelId) ??
			null
		);
	}, [selectedModelId, selectedProvider]);
	return { selectedProvider, selectedModel };
}

export function useOpenModelPickerShortcut({
	disabled,
	hasProviders,
	onOpen,
}: {
	disabled: boolean;
	hasProviders: boolean;
	onOpen: () => void;
}) {
	const onOpenRef = useRef(onOpen);
	onOpenRef.current = onOpen;
	useEffect(() => {
		const open = () => {
			if (!disabled && hasProviders) onOpenRef.current();
		};
		window.addEventListener(DCC_OPEN_MODEL_PICKER_EVENT, open);
		return () => window.removeEventListener(DCC_OPEN_MODEL_PICKER_EVENT, open);
	}, [disabled, hasProviders]);
}

/** One line per usage window; detail lives in Settings → Providers. */
export function AccountUsageStrip({
	accountUsage,
	isFetching,
	hasError,
	onRefresh,
}: {
	accountUsage: ProviderAccountUsage | null;
	isFetching: boolean;
	hasError: boolean;
	onRefresh?: () => void;
}) {
	const { t, i18n } = useTranslation("common");
	const windows = accountUsage?.state === "available" ? accountUsage.windows : [];

	return (
		<div className="flex shrink-0 items-center gap-2 border-t border-border/60 px-3 py-2">
			<div className="min-w-0 flex-1">
				{windows.length ? (
					<div className="flex items-center gap-3">
						{windows.map((window) => {
							const severity = providerUsageSeverity(window);
							const reset = formatProviderUsageReset(window, i18n.language);
							const name = window.windowDurationMinutes === 300
								? t("composer.accountUsage.fiveHour")
								: window.windowDurationMinutes === 10_080
									? t("composer.accountUsage.sevenDay")
									: window.id.replaceAll("_", " ");
							const remaining = Math.round(window.remainingPercent);
							return (
								<div
									key={window.id}
									className="min-w-0 flex-1"
									title={reset ? t("composer.accountUsage.nextReset", { date: reset }) : undefined}
								>
									<div className="flex items-baseline justify-between gap-2 text-[10px]">
										<span className="truncate text-muted-foreground">{name}</span>
										<span
											className={cn(
												"shrink-0 font-medium tabular-nums",
												severity === "warning" && "text-amber-600 dark:text-amber-400",
												severity === "critical" && "text-destructive",
											)}
										>
											{t("composer.accountUsage.remaining", { percent: remaining })}
										</span>
									</div>
									<div className="mt-1 h-1 overflow-hidden rounded-full bg-muted">
										<div
											className={cn(
												"h-full rounded-full bg-foreground/40",
												severity === "warning" && "bg-amber-500",
												severity === "critical" && "bg-destructive",
											)}
											style={{ width: `${Math.min(100, Math.max(0, remaining))}%` }}
										/>
									</div>
								</div>
							);
						})}
					</div>
				) : (
					<p className={cn("truncate text-[11px]", hasError ? "text-destructive" : "text-muted-foreground")}>
						{accountUsage?.state === "awaitingActivity"
							? t("composer.accountUsage.awaitingActivity")
							: hasError
								? t("composer.accountUsage.error")
								: isFetching
									? t("composer.accountUsage.loading")
									: t("composer.accountUsage.title")}
					</p>
				)}
			</div>
			<button
				type="button"
				className="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
				aria-label={t("composer.accountUsage.refresh")}
				disabled={isFetching}
				onClick={onRefresh}
			>
				{isFetching ? (
					<LoaderCircle className="size-3 animate-spin" />
				) : (
					<RefreshCcw className="size-3" />
				)}
			</button>
		</div>
	);
}

