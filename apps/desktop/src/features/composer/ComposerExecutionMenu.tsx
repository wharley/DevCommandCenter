import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	Check,
	ChevronDown,
	ChevronRight,
	LoaderCircle,
	RefreshCcw,
	Settings2,
	Star,
} from "lucide-react";
import type { ProviderAccountUsage, ProviderCatalog } from "@dcc/contracts";
import { ProviderIcon } from "@/features/providers/provider-icons";
import {
	Command,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
} from "@/components/ui/command";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuGroup,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuSub,
	DropdownMenuSubContent,
	DropdownMenuSubTrigger,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import {
	providerUsageSeverity,
	supportsProviderAccountUsage,
} from "@/features/providers/provider-account-usage";
import { isProviderEnabled } from "@/features/providers/provider-selection.logic";
import {
	composerToolbarTriggerClassName,
	getCompactComposerModelLabel,
} from "./WorkspaceComposer.logic";
import { EffortBrainIcon } from "./EffortBrainIcon";
import { DEFAULT_EFFORT_LEVEL, getEffortDisplay } from "./effort";
import { ModelFavoritesDialog } from "./ModelFavoritesDialog";
import {
	addModelFavorite, modelFavoriteKey, resolveModelFavorite, useModelFavorites,
	type ModelFavorite,
} from "./model-favorites";

export const DCC_OPEN_MODEL_PICKER_EVENT = "dcc:open-model-picker";

type ComposerProvider = ProviderCatalog["providers"][number];
type ComposerModel = ComposerProvider["models"][number];

type ComposerExecutionMenuProps = {
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

export function ComposerExecutionMenu({
	open,
	onOpenChange,
	providers,
	selectedProviderId,
	selectedModelId,
	availableEffortLevels,
	selectedEffortId,
	directResponse,
	onSelectProvider,
	onSelectModel,
	onSelectEffort,
	onSelectUltrathink,
	onSetDirectResponse,
	accountUsage = null,
	isAccountUsageFetching = false,
	hasAccountUsageError = false,
	onRefreshAccountUsage,
	disabled = false,
}: ComposerExecutionMenuProps) {
	const { t } = useTranslation("common");
	const [modelSubOpen, setModelSubOpen] = useState(false);
	const [modelSearch, setModelSearch] = useState("");
	const [cursorAdvancedOpen, setCursorAdvancedOpen] = useState(false);
	const [editingFavorites, setEditingFavorites] = useState(false);
	const triggerRef = useRef<HTMLButtonElement>(null);
	const favorites = useModelFavorites();

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
	const currentFavorite: ModelFavorite | null = selectedProvider && selectedModel
		? { providerId: selectedProvider.id, modelId: selectedModel.id,
			effort: selectedModel.effortLevels.length ? selectedEffortId : null }
		: null;
	const currentFavoriteSaved = currentFavorite && favorites.some((favorite) =>
		modelFavoriteKey(favorite) === modelFavoriteKey(currentFavorite));

	const compactModelLabel = selectedModel
		? getCompactComposerModelLabel(selectedProvider?.id ?? null, selectedModel.label)
		: (selectedModelId ?? t("composer.model.select"));
	const effortDisplay = getEffortDisplay(selectedEffortId);
	const effortLabel = t(`composer.effort.${selectedEffortId}`, {
		defaultValue: effortDisplay.label,
	});
	const responseLabel = directResponse
		? t("composer.execution.response.direct")
		: t("composer.execution.response.standard");
	const triggerTitle = selectedModel
		? `${selectedModel.label} — ${selectedProvider?.label ?? "Provider"} · ${effortLabel}`
		: compactModelLabel;

	useEffect(() => {
		const openModelPicker = () => {
			if (disabled || providers.length === 0) return;
			onOpenChange(true);
			setModelSubOpen(favorites.length === 0);
			onRefreshAccountUsage?.();
		};
		window.addEventListener(DCC_OPEN_MODEL_PICKER_EVENT, openModelPicker);
		return () =>
			window.removeEventListener(DCC_OPEN_MODEL_PICKER_EVENT, openModelPicker);
	}, [disabled, onOpenChange, onRefreshAccountUsage, providers.length, favorites.length]);

	const closeModelMenu = () => {
		setModelSubOpen(false);
		setModelSearch("");
		setCursorAdvancedOpen(false);
	};

	const renderModelItem = (provider: ComposerProvider, model: ComposerModel) => {
		const isActive =
			provider.id === selectedProvider?.id && model.id === selectedModelId;
		return (
			<CommandItem
				key={`${provider.id}-${model.id}`}
				value={`${provider.label} ${model.label} ${model.id} ${model.description}`}
				disabled={disabled || !isProviderEnabled(provider)}
				onSelect={() => {
					if (provider.id !== selectedProviderId) {
						onSelectProvider(provider.id);
					}
					onSelectModel(model.id);
					closeModelMenu();
					onOpenChange(false);
				}}
				className="[&>svg:last-child]:hidden flex items-center gap-2 font-mono text-[13px] tabular-nums"
			>
				<ProviderIcon provider={provider.id} className="size-4" />
				<span className="min-w-0 flex-1 truncate">{model.label}</span>
				{!isProviderEnabled(provider) ? (
					<span className="text-[10px] text-muted-foreground">
						{t("settings.model.disabled")}
					</span>
				) : null}
				{isActive ? (
					<Check className="size-4 shrink-0" strokeWidth={2} />
				) : (
					<span className="size-4 shrink-0" aria-hidden />
				)}
			</CommandItem>
		);
	};

	const hasModelSearch = modelSearch.trim().length > 0;

	return (
		<>
		<DropdownMenu
			open={open}
			onOpenChange={(nextOpen) => {
				onOpenChange(nextOpen);
				if (nextOpen) onRefreshAccountUsage?.();
				if (!nextOpen) closeModelMenu();
			}}
		>
			<DropdownMenuTrigger
				ref={triggerRef}
				type="button"
				disabled={disabled || providers.length === 0}
				title={triggerTitle}
				aria-label={t("composer.execution.openWithSelection", {
					model: compactModelLabel,
					effort: effortLabel,
				})}
				className={cn(
					`flex h-7 min-w-0 max-w-[14rem] items-center gap-1.5 ${composerToolbarTriggerClassName}`,
					"text-muted-foreground",
					disabled &&
						"cursor-not-allowed opacity-45 hover:bg-transparent hover:text-muted-foreground",
				)}
			>
				<ProviderIcon
					provider={selectedProvider?.id ?? selectedProvider?.label}
					className="size-[13px] shrink-0"
				/>
				<span className="dcc-composer-model-summary min-w-0 truncate text-[12px] font-medium leading-4 text-foreground">
					{compactModelLabel}
				</span>
				<span className="dcc-composer-effort-summary shrink-0 text-[12px] leading-4 text-muted-foreground">
					· {effortLabel}
				</span>
				<ChevronDown className="size-3 shrink-0 opacity-40" strokeWidth={2} />
			</DropdownMenuTrigger>

			<DropdownMenuContent side="top" align="end" sideOffset={4}
				className="flex w-80 max-w-[calc(100vw-2rem)] flex-col"
				onCloseAutoFocus={(event) => { if (editingFavorites) event.preventDefault(); }}>
				<DropdownMenuLabel className="shrink-0">{t("composer.favorites.title")}</DropdownMenuLabel>
				<DropdownMenuGroup
					aria-label={t("composer.favorites.title")}
					className="min-h-0 max-h-64 shrink overflow-y-auto overscroll-contain"
				>
				{favorites.length ? favorites.map((favorite) => {
					const { provider, model, available } = resolveModelFavorite(favorite, providers);
					const isActive = currentFavorite && modelFavoriteKey(currentFavorite) === modelFavoriteKey(favorite);
					const label = favorite.effort === null ? t("composer.favorites.managed")
						: t(`composer.effort.${favorite.effort}`, { defaultValue: getEffortDisplay(favorite.effort).label });
					const description = `${model?.label ?? favorite.modelId} · ${provider?.label ?? favorite.providerId} · ${label}${
						available ? "" : ` · ${t("composer.favorites.unavailable")}`
					}`;
					return <DropdownMenuItem key={modelFavoriteKey(favorite)}
						disabled={disabled || !available}
						textValue={`${model?.label ?? favorite.modelId} ${label}`}
						title={description}
						aria-label={description}
						className="h-8 gap-2 py-1"
						onSelect={() => {
							if (disabled || !available) return;
							if (favorite.providerId !== selectedProviderId) onSelectProvider(favorite.providerId);
							onSelectModel(favorite.modelId);
							if (favorite.effort === "ultrathink") onSelectUltrathink();
							else onSelectEffort(favorite.effort ?? DEFAULT_EFFORT_LEVEL);
							closeModelMenu();
							onOpenChange(false);
						}}>
						<ProviderIcon provider={favorite.providerId} className="size-4 shrink-0" />
						<span className="min-w-0 flex-1 truncate text-[13px]">
							{model?.label ?? favorite.modelId}
							{!available && <span className="text-[11px] text-muted-foreground">
								{` · ${t("composer.favorites.unavailable")}`}
							</span>}
						</span>
						<span className="shrink-0 text-[11px] text-muted-foreground">{label}</span>
						{isActive ? <Check className="size-3.5 shrink-0" /> : <span className="size-3.5 shrink-0" />}
					</DropdownMenuItem>;
				}) : <p className="px-1.5 py-2 text-xs text-muted-foreground">{t("composer.favorites.emptyMenu")}</p>}
				</DropdownMenuGroup>
				<div className="shrink-0">
				<DropdownMenuSeparator />

				<DropdownMenuSub
					open={modelSubOpen}
					onOpenChange={(nextOpen) => {
						setModelSubOpen(nextOpen);
						if (!nextOpen) {
							setModelSearch("");
							setCursorAdvancedOpen(false);
						}
					}}
				>
					<DropdownMenuSubTrigger className="justify-between gap-3">
						<span>{t("composer.favorites.allModels")}</span>
						<ChevronRight className="size-3.5 shrink-0 opacity-50" />
					</DropdownMenuSubTrigger>
					<DropdownMenuSubContent
						sideOffset={6}
						className="w-[min(22rem,calc(100vw-2rem))] max-w-[calc(100vw-2rem)] p-0"
					>
						<Command className="rounded-lg border-0 shadow-none">
							<CommandInput
								placeholder={t("composer.model.search")}
								className="h-9"
								value={modelSearch}
								onValueChange={setModelSearch}
								onKeyDown={(event) => event.stopPropagation()}
							/>
							<CommandList>
								<CommandEmpty>{t("composer.model.empty")}</CommandEmpty>
								{providers.map((provider) => {
									const isCursor = provider.id === "cursor";
					const primaryModels = isCursor
						? provider.models.filter(
								(model) => model.id.trim().toLowerCase() === "auto",
							)
						: provider.models;
					const advancedModels = isCursor
						? provider.models.filter(
								(model) =>
									model.id.trim().toLowerCase() !== "auto" &&
									!model.id.includes(" - "),
							)
										: [];
									const revealAdvanced = cursorAdvancedOpen || hasModelSearch;

									return (
										<Fragment key={provider.id}>
											<CommandGroup heading={provider.label}>
												{primaryModels.map((model) =>
													renderModelItem(provider, model),
												)}
												{isCursor && advancedModels.length > 0 && !hasModelSearch ? (
													<CommandItem
														value={t("composer.model.cursorAdvanced")}
														onSelect={() =>
															setCursorAdvancedOpen((current) => !current)
														}
														className="[&>svg:last-child]:hidden flex items-center gap-2 text-[12px] text-muted-foreground"
													>
														<ProviderIcon provider={provider.id} className="size-4" />
														<span className="min-w-0 flex-1">
															{t("composer.model.cursorAdvanced")}
														</span>
														{cursorAdvancedOpen ? (
															<ChevronDown className="size-3.5" />
														) : (
															<ChevronRight className="size-3.5" />
														)}
													</CommandItem>
												) : null}
											</CommandGroup>
											{isCursor && revealAdvanced && advancedModels.length > 0 ? (
												<CommandGroup heading={t("composer.model.cursorAdvancedHeading")}>
													{advancedModels.map((model) =>
														renderModelItem(provider, model),
													)}
												</CommandGroup>
											) : null}
										</Fragment>
									);
								})}
							</CommandList>
						</Command>
					</DropdownMenuSubContent>
				</DropdownMenuSub>

				<DropdownMenuItem disabled={disabled || !currentFavorite || Boolean(currentFavoriteSaved) ||
					Boolean(currentFavorite && !resolveModelFavorite(currentFavorite, providers).available)}
					onSelect={(event) => {
						event.preventDefault();
						if (currentFavorite) addModelFavorite(currentFavorite);
					}}>
					<Star className="size-3.5" />
					{t(currentFavoriteSaved ? "composer.favorites.saved" : "composer.favorites.saveCurrent")}
				</DropdownMenuItem>
				<DropdownMenuItem onSelect={() => { closeModelMenu(); setEditingFavorites(true); }}>
					<Settings2 className="size-3.5" />{t("composer.favorites.edit")}
				</DropdownMenuItem>
				<DropdownMenuSeparator />

				<DropdownMenuSub>
					<DropdownMenuSubTrigger className="justify-between gap-3">
						<span>{t("composer.execution.effort")}</span>
						<span className="ml-auto text-[12px] text-muted-foreground">{effortLabel}</span>
						<ChevronRight className="size-3.5 shrink-0 opacity-50" />
					</DropdownMenuSubTrigger>
					<DropdownMenuSubContent sideOffset={6} className="w-56">
						<DropdownMenuLabel>{t("composer.execution.effort")}</DropdownMenuLabel>
						{availableEffortLevels.map((id) => {
							const display = getEffortDisplay(id);
							return (
								<DropdownMenuItem
									key={id}
									className="justify-between gap-3"
									onClick={() => onSelectEffort(id)}
								>
									<span className="flex items-center gap-2.5">
										<EffortBrainIcon level={display.icon} />
										{t(`composer.effort.${id}`, { defaultValue: display.label })}
									</span>
									{selectedEffortId === id ? <Check className="size-4" /> : null}
								</DropdownMenuItem>
							);
						})}
						<DropdownMenuItem
							className="justify-between gap-3"
							onClick={onSelectUltrathink}
						>
							<span className="flex items-center gap-2.5">
								<EffortBrainIcon level="max" />
								{t("composer.effort.ultrathink")}
							</span>
							{selectedEffortId === "ultrathink" ? <Check className="size-4" /> : null}
						</DropdownMenuItem>
					</DropdownMenuSubContent>
				</DropdownMenuSub>

				<DropdownMenuSub>
					<DropdownMenuSubTrigger className="justify-between gap-3">
						<span>{t("composer.execution.response.title")}</span>
						<span className="ml-auto text-[12px] text-muted-foreground">{responseLabel}</span>
						<ChevronRight className="size-3.5 shrink-0 opacity-50" />
					</DropdownMenuSubTrigger>
					<DropdownMenuSubContent sideOffset={6} className="w-64">
						{([false, true] as const).map((direct) => (
							<DropdownMenuItem
								key={String(direct)}
								className="items-start justify-between gap-3 py-2"
								onClick={() => onSetDirectResponse(direct)}
							>
								<span>
									<span className="block">
										{direct
											? t("composer.execution.response.direct")
											: t("composer.execution.response.standard")}
									</span>
									<span className="mt-0.5 block text-[12px] leading-4 text-muted-foreground">
										{direct
											? t("composer.execution.response.directHint")
											: t("composer.execution.response.standardHint")}
										{direct &&
										selectedProvider?.capabilities.fastModeSupport === "prompt_fallback"
											? ` ${t("composer.execution.response.directPromptFallback")}`
											: ""}
									</span>
								</span>
								{directResponse === direct ? <Check className="mt-0.5 size-4" /> : null}
							</DropdownMenuItem>
						))}
					</DropdownMenuSubContent>
				</DropdownMenuSub>

				{supportsProviderAccountUsage(selectedProvider) ? (
					<>
						<DropdownMenuSeparator />
						<div className="px-1.5 py-1.5">
							<div className="mb-1.5 flex items-center justify-between gap-3">
								<span className="text-[11px] font-medium text-foreground">
									{t("composer.accountUsage.title")}
								</span>
								<button
									type="button"
									className="rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
									aria-label={t("composer.accountUsage.refresh")}
									disabled={isAccountUsageFetching}
									onClick={onRefreshAccountUsage}
								>
									{isAccountUsageFetching ? (
										<LoaderCircle className="size-3 animate-spin" />
									) : (
										<RefreshCcw className="size-3" />
									)}
								</button>
							</div>
							{accountUsage?.state === "available" && accountUsage.windows.length ? (
								<div className="space-y-1">
									{accountUsage.windows.map((window) => {
										const severity = providerUsageSeverity(window);
										return (
											<div
												key={window.id}
												className="flex items-center justify-between gap-3 text-[11px]"
											>
												<span className="truncate text-muted-foreground">
													{window.windowDurationMinutes === 300
														? t("composer.accountUsage.fiveHour")
														: window.windowDurationMinutes === 10_080
															? t("composer.accountUsage.sevenDay")
															: window.id.replaceAll("_", " ")}
												</span>
												<span
													className={cn(
														"shrink-0 font-medium tabular-nums",
														severity === "warning" && "text-amber-600 dark:text-amber-400",
														severity === "critical" && "text-destructive",
													)}
												>
													{t("composer.accountUsage.remaining", {
														percent: Math.round(window.remainingPercent),
													})}
												</span>
											</div>
										);
									})}
								</div>
							) : accountUsage?.state === "awaitingActivity" ? (
								<p className="text-[11px] leading-relaxed text-muted-foreground">
									{t("composer.accountUsage.awaitingActivity")}
								</p>
							) : hasAccountUsageError ? (
								<p className="text-[11px] text-destructive">
									{t("composer.accountUsage.error")}
								</p>
							) : (
								<p className="text-[11px] text-muted-foreground">
									{isAccountUsageFetching
										? t("composer.accountUsage.loading")
										: t("composer.accountUsage.openToLoad")}
								</p>
							)}
						</div>
					</>
				) : null}
				</div>
			</DropdownMenuContent>
		</DropdownMenu>
		{editingFavorites && <ModelFavoritesDialog providers={providers} initialFavorite={currentFavorite}
			onClose={() => setEditingFavorites(false)} onRestoreFocus={() => triggerRef.current?.focus()} />}
		</>
	);
}
