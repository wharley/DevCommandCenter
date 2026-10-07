import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown } from "lucide-react";
import { ProviderIcon } from "@/features/providers/provider-icons";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import { supportsProviderAccountUsage } from "@/features/providers/provider-account-usage";
import {
	composerToolbarTriggerClassName,
	getCompactComposerModelLabel,
} from "./WorkspaceComposer.logic";
import { getEffortDisplay } from "./effort";
import { ModelFavoritesDialog } from "./ModelFavoritesDialog";
import { ModelPickerPanel } from "./ModelPickerPanel";
import type { ModelFavorite } from "./model-favorites";
import { CompactExecutionPicker } from "./CompactExecutionPicker";
import {
	AccountUsageStrip,
	useComposerSelection,
	useOpenModelPickerShortcut,
	type ComposerExecutionMenuProps,
} from "./ComposerExecutionShared";

export {
	DCC_OPEN_MODEL_PICKER_EVENT,
	type ComposerExecutionMenuProps,
} from "./ComposerExecutionShared";

export function ComposerExecutionMenu(props: ComposerExecutionMenuProps) {
	return props.compact ? (
		<CompactExecutionPicker {...props} />
	) : (
		<FullExecutionMenu {...props} />
	);
}

function FullExecutionMenu({
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
	const [editingFavorites, setEditingFavorites] = useState(false);
	const triggerRef = useRef<HTMLButtonElement>(null);
	const { selectedProvider, selectedModel } = useComposerSelection(
		providers,
		selectedProviderId,
		selectedModelId,
	);
	const currentFavorite: ModelFavorite | null = selectedProvider && selectedModel
		? { providerId: selectedProvider.id, modelId: selectedModel.id,
			effort: selectedModel.effortLevels.length ? selectedEffortId : null }
		: null;

	const compactModelLabel = selectedModel
		? getCompactComposerModelLabel(selectedProvider?.id ?? null, selectedModel.label)
		: (selectedModelId ?? t("composer.model.select"));
	const effortLabel = t(`composer.effort.${selectedEffortId}`, {
		defaultValue: getEffortDisplay(selectedEffortId).label,
	});
	const triggerTitle = selectedModel
		? `${selectedModel.label} — ${selectedProvider?.label ?? "Provider"} · ${effortLabel}`
		: compactModelLabel;

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
		<Popover
			open={open}
			onOpenChange={(nextOpen) => {
				onOpenChange(nextOpen);
				if (nextOpen) onRefreshAccountUsage?.();
			}}
		>
			<PopoverTrigger
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
				{availableEffortLevels.length > 0 ? (
					<span className="dcc-composer-effort-summary shrink-0 text-[12px] leading-4 text-muted-foreground">
						· {effortLabel}
					</span>
				) : null}
				<ChevronDown className="size-3 shrink-0 opacity-40" strokeWidth={2} />
			</PopoverTrigger>

			<PopoverContent
				side="top"
				align="end"
				sideOffset={6}
				collisionPadding={12}
				className="w-[min(20rem,calc(100vw-2rem))] gap-0 overflow-hidden rounded-2xl p-0 shadow-lg"
				onOpenAutoFocus={(event) => event.preventDefault()}
				onCloseAutoFocus={(event) => { if (editingFavorites) event.preventDefault(); }}
			>
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
			</PopoverContent>
		</Popover>
		{editingFavorites && <ModelFavoritesDialog providers={providers} initialFavorite={currentFavorite}
			onClose={() => setEditingFavorites(false)} onRestoreFocus={() => triggerRef.current?.focus()} />}
		</>
	);
}
