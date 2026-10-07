import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown } from "lucide-react";
import {
	Dialog,
	DialogContent,
	DialogTitle,
	DialogTrigger,
} from "@/components/ui/dialog";
import { ProviderIcon } from "@/features/providers/provider-icons";
import { supportsProviderAccountUsage } from "@/features/providers/provider-account-usage";
import {
	composerToolbarTriggerClassName,
	getCompactComposerModelLabel,
} from "./WorkspaceComposer.logic";
import { getEffortDisplay } from "./effort";
import { ModelFavoritesDialog } from "./ModelFavoritesDialog";
import { ModelPickerPanel } from "./ModelPickerPanel";
import {
	AccountUsageStrip,
	useComposerSelection,
	useOpenModelPickerShortcut,
	type ComposerExecutionMenuProps,
} from "./ComposerExecutionShared";

/**
 * Small windows get the same picker as a centered dialog, so it never has to
 * squeeze beside the composer.
 */
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
	accountUsage = null,
	isAccountUsageFetching = false,
	hasAccountUsageError = false,
	disabled = false,
}: ComposerExecutionMenuProps) {
	const { t } = useTranslation("common");
	const [editingFavorites, setEditingFavorites] = useState(false);
	const { selectedProvider, selectedModel } = useComposerSelection(
		providers,
		selectedProviderId,
		selectedModelId,
	);
	const label = selectedModel
		? getCompactComposerModelLabel(selectedProvider?.id ?? null, selectedModel.label)
		: selectedModelId || t("composer.model.select");
	const effortLabel = t(`composer.effort.${selectedEffortId}`, {
		defaultValue: getEffortDisplay(selectedEffortId).label,
	});

	useOpenModelPickerShortcut({
		disabled,
		hasProviders: providers.length > 0,
		onOpen: () => {
			onOpenChange(true);
			onRefreshAccountUsage?.();
		},
	});

	return (
		<>
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
						effort: effortLabel,
					})}
					className={`flex h-7 min-w-0 max-w-[14rem] items-center gap-1.5 text-muted-foreground ${composerToolbarTriggerClassName}`}
				>
					<ProviderIcon provider={selectedProvider?.id} className="size-[13px] shrink-0" />
					<span className="truncate text-xs text-foreground">{label}</span>
					{availableEffortLevels.length > 0 && (
						<span className="shrink-0 text-xs">· {effortLabel}</span>
					)}
					<ChevronDown className="size-3 shrink-0" />
				</button>
			</DialogTrigger>
			<DialogContent
				aria-describedby={undefined}
				showCloseButton={false}
				onCloseAutoFocus={(event) => {
					if (editingFavorites) {
						event.preventDefault();
					} else if (onReturnToComposer) {
						event.preventDefault();
						onReturnToComposer();
					}
				}}
				className="flex max-h-[calc(100dvh-2rem)] w-[min(22rem,calc(100vw-2rem))] max-w-[22rem] flex-col gap-0 overflow-hidden rounded-2xl p-0 sm:max-w-[22rem]"
			>
				<DialogTitle className="sr-only">{t("composer.execution.compactTitle")}</DialogTitle>
				<ModelPickerPanel
					providers={providers}
					selectedProviderId={selectedProvider?.id ?? selectedProviderId}
					selectedModelId={selectedModelId}
					availableEffortLevels={availableEffortLevels}
					selectedEffortId={selectedEffortId}
					directResponse={directResponse}
					disabled={disabled}
					onSelectProvider={onSelectProvider}
					onSelectModel={onSelectModel}
					onSelectEffort={onSelectEffort}
					onSelectUltrathink={onSelectUltrathink}
					onSetDirectResponse={onSetDirectResponse}
					onDone={() => onOpenChange(false)}
					onEditFavorites={() => {
						onOpenChange(false);
						setEditingFavorites(true);
					}}
					footer={supportsProviderAccountUsage(selectedProvider) ? (
						<AccountUsageStrip
							accountUsage={accountUsage}
							isFetching={isAccountUsageFetching}
							hasError={hasAccountUsageError}
							onRefresh={onRefreshAccountUsage}
						/>
					) : null}
				/>
			</DialogContent>
		</Dialog>
		{editingFavorites && (
			<ModelFavoritesDialog
				providers={providers}
				initialFavorite={
					selectedProvider && selectedModel
						? {
								providerId: selectedProvider.id,
								modelId: selectedModel.id,
								effort: selectedModel.effortLevels.length ? selectedEffortId : null,
							}
						: null
				}
				onClose={() => setEditingFavorites(false)}
				onRestoreFocus={() => onReturnToComposer?.()}
			/>
		)}
		</>
	);
}
