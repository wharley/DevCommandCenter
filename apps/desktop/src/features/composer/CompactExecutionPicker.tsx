import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Check, ChevronDown, Star } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogTitle,
	DialogTrigger,
} from "@/components/ui/dialog";
import {
	Command,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
} from "@/components/ui/command";
import { ProviderIcon } from "@/features/providers/provider-icons";
import { isProviderEnabled } from "@/features/providers/provider-selection.logic";
import {
	composerToolbarTriggerClassName,
	getCompactComposerModelLabel,
} from "./WorkspaceComposer.logic";
import { DEFAULT_EFFORT_LEVEL, getEffortDisplay } from "./effort";
import {
	addModelFavorite,
	removeModelFavorite,
	modelFavoriteKey,
	resolveModelFavorite,
	useModelFavorites,
	type ModelFavorite,
} from "./model-favorites";
import type { ComposerExecutionMenuProps } from "./ComposerExecutionMenu";

/** A single bounded surface for small windows; no sideways submenus. */
export function CompactExecutionPicker({
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
	onRefreshAccountUsage,
	onReturnToComposer,
	accountUsage,
	disabled = false,
}: ComposerExecutionMenuProps) {
	const { t } = useTranslation("common");
	const favorites = useModelFavorites();
	const provider = providers.find((entry) => entry.id === selectedProviderId);
	const model = provider?.models.find((entry) => entry.id === selectedModelId);
	const label = model
		? getCompactComposerModelLabel(provider?.id ?? null, model.label)
		: selectedModelId || t("composer.model.select");
	const effortLabel = (effort: string) =>
		t(`composer.effort.${effort}`, {
			defaultValue: getEffortDisplay(effort).label,
		});
	const current: ModelFavorite | null =
		provider && model
			? {
					providerId: provider.id,
					modelId: model.id,
					effort: model.effortLevels.length ? selectedEffortId : null,
				}
			: null;
	const saved =
		current &&
		favorites.some(
			(entry) => modelFavoriteKey(entry) === modelFavoriteKey(current),
		);
	const select = (providerId: string, modelId: string) => {
		if (providerId !== selectedProviderId) onSelectProvider(providerId);
		onSelectModel(modelId);
	};
	useEffect(() => {
		const show = () => {
			if (!disabled && providers.length) {
				onOpenChange(true);
				onRefreshAccountUsage?.();
			}
		};
		window.addEventListener("dcc:open-model-picker", show);
		return () => window.removeEventListener("dcc:open-model-picker", show);
	}, [disabled, providers.length, onOpenChange, onRefreshAccountUsage]);
	return (
		<Dialog
			open={open}
			onOpenChange={(next) => {
				onOpenChange(next);
				if (next) onRefreshAccountUsage?.();
			}}
		>
			<DialogTrigger asChild>
				<button
					type="button"
					disabled={disabled || !providers.length}
					aria-label={t("composer.execution.openWithSelection", {
						model: label,
						effort: effortLabel(selectedEffortId),
					})}
					className={`flex h-7 min-w-0 max-w-[14rem] items-center gap-1.5 text-muted-foreground ${composerToolbarTriggerClassName}`}
				>
					<ProviderIcon
						provider={provider?.id}
						className="size-[13px] shrink-0"
					/>
					<span className="truncate text-xs text-foreground">{label}</span>
					{availableEffortLevels.length > 0 && (
						<span className="shrink-0 text-xs">
							· {effortLabel(selectedEffortId)}
						</span>
					)}
					<ChevronDown className="size-3 shrink-0" />
				</button>
			</DialogTrigger>
			<DialogContent
				aria-describedby={undefined}
				onCloseAutoFocus={(event) => {
					if (onReturnToComposer) {
						event.preventDefault();
						onReturnToComposer();
					}
				}}
				className="flex h-[calc(100dvh-2rem)] max-h-[400px] w-[calc(100vw-2rem)] max-w-[660px] flex-col gap-0 overflow-hidden p-0 sm:max-w-[660px]"
			>
				<div className="shrink-0 border-b px-4 py-3 pr-12">
					<DialogTitle className="text-sm">
						{t("composer.execution.compactTitle")}
					</DialogTitle>
				</div>
				<div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_minmax(0,0.75fr)]">
					<Command className="min-h-0 rounded-none p-2">
						<CommandInput
							placeholder={t("composer.model.search")}
							aria-label={t("composer.model.search")}
						/>
						<CommandList className="min-h-0 max-h-none flex-1 overscroll-contain">
							<CommandEmpty>{t("composer.model.empty")}</CommandEmpty>
							{favorites.length > 0 && (
								<CommandGroup heading={t("composer.favorites.title")}>
									{favorites.map((favorite) => {
										const resolved = resolveModelFavorite(favorite, providers);
										const effort = favorite.effort
											? effortLabel(favorite.effort)
											: t("composer.favorites.managed");
										return (
											<CommandItem
												key={modelFavoriteKey(favorite)}
												value={`favorite ${favorite.providerId} ${favorite.modelId} ${resolved.model?.label} ${effort}`}
												disabled={disabled || !resolved.available}
												onSelect={() => {
													select(favorite.providerId, favorite.modelId);
													if (favorite.effort === "ultrathink")
														onSelectUltrathink();
													else
														onSelectEffort(
															favorite.effort ?? DEFAULT_EFFORT_LEVEL,
														);
												}}
												className="gap-2 [&>svg:last-child]:hidden"
											>
												<Star className="size-3.5 shrink-0" />
												<span className="min-w-0 flex-1 truncate">
													{resolved.model?.label ?? favorite.modelId}
												</span>
												<span className="shrink-0 text-[10px] text-muted-foreground">
													{effort}
												</span>
											</CommandItem>
										);
									})}
								</CommandGroup>
							)}
							{providers.map((entry) => (
								<CommandGroup key={entry.id} heading={entry.label}>
									{entry.models.map((choice) => (
										<CommandItem
											key={choice.id}
											value={`${entry.id} ${entry.label} ${choice.id} ${choice.label}`}
											disabled={disabled || !isProviderEnabled(entry)}
											onSelect={() => select(entry.id, choice.id)}
											className="gap-2 [&>svg:last-child]:hidden"
										>
											<ProviderIcon
												provider={entry.id}
												className="size-3.5 shrink-0"
											/>
											<span
												className="min-w-0 flex-1 truncate"
												title={choice.label}
											>
												{choice.label}
											</span>
											{entry.id === selectedProviderId &&
												choice.id === selectedModelId && (
													<span>
														<Check className="size-3.5" />
													</span>
												)}
										</CommandItem>
									))}
								</CommandGroup>
							))}
						</CommandList>
					</Command>
					<div className="min-h-0 space-y-4 overflow-y-auto border-l bg-muted/20 p-4">
						<div className="space-y-1">
							<p className="break-words text-sm font-medium">
								{model?.label ?? label}
							</p>
							<p className="text-xs text-muted-foreground">{provider?.label}</p>
						</div>
						{availableEffortLevels.length > 0 && (
							<label className="block space-y-1.5 text-xs font-medium">
								<span>{t("composer.execution.effort")}</span>
								<select
									aria-label={t("composer.execution.effort")}
									value={selectedEffortId}
									disabled={disabled}
									className="h-8 w-full rounded-md border bg-background px-2 text-xs"
									onChange={(event) =>
										event.target.value === "ultrathink"
											? onSelectUltrathink()
											: onSelectEffort(event.target.value)
									}
								>
									{[...new Set([...availableEffortLevels, "ultrathink"])].map(
										(level) => (
											<option key={level} value={level}>
												{effortLabel(level)}
											</option>
										),
									)}
								</select>
							</label>
						)}
						<label className="block space-y-1.5 text-xs font-medium">
							<span>{t("composer.execution.response.title")}</span>
							<select
								aria-label={t("composer.execution.response.title")}
								value={String(directResponse)}
								disabled={disabled}
								className="h-8 w-full rounded-md border bg-background px-2 text-xs"
								onChange={(event) =>
									onSetDirectResponse(event.target.value === "true")
								}
							>
								<option value="false">
									{t("composer.execution.response.standard")}
								</option>
								<option value="true">
									{t("composer.execution.response.direct")}
								</option>
							</select>
						</label>
						<Button
							variant="outline"
							size="sm"
							className="h-auto w-full whitespace-normal py-2 text-xs"
							disabled={
								disabled ||
								!current ||
								!resolveModelFavorite(current, providers).available
							}
							onClick={() => {
								if (current)
									saved
										? removeModelFavorite(current)
										: addModelFavorite(current);
							}}
						>
							<Star
								className="size-3.5 shrink-0"
								fill={saved ? "currentColor" : "none"}
							/>
							{t(
								saved
									? "composer.execution.removeFavorite"
									: "composer.favorites.saveCurrent",
							)}
						</Button>
						{accountUsage?.state === "available" &&
							accountUsage.windows.map((window) => (
								<p key={window.id} className="text-xs text-muted-foreground">
									{t(window.windowDurationMinutes === 300
										? "composer.accountUsage.fiveHour"
										: window.windowDurationMinutes === 10_080
											? "composer.accountUsage.sevenDay"
											: "composer.accountUsage.title")} ·{" "}
									{t("composer.accountUsage.remaining", {
										percent: Math.round(window.remainingPercent),
									})}
								</p>
							))}
					</div>
				</div>
				<div className="flex shrink-0 justify-end border-t px-3 py-2">
					<Button size="sm" onClick={() => onOpenChange(false)}>
						{t("composer.execution.backToPrompt")}
					</Button>
				</div>
			</DialogContent>
		</Dialog>
	);
}
