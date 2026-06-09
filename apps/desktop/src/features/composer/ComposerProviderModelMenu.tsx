import { useEffect, useMemo, useState } from "react";
import { CheckIcon, ChevronDown, Sparkles } from "lucide-react";
import type { ProviderCatalog } from "@dcc/contracts";
import {
	Command,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
} from "@/components/ui/command";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import { composerToolbarTriggerClassName } from "./WorkspaceComposer.logic";

export const DCC_OPEN_MODEL_PICKER_EVENT = "dcc:open-model-picker";

type ComposerProviderModelMenuProps = {
	providers: ProviderCatalog["providers"];
	selectedProviderId: string | null;
	selectedModelId: string | null;
	onSelectProvider: (providerId: string) => void;
	onSelectModel: (modelId: string) => void;
	disabled?: boolean;
};

/**
 * Popover + cmdk search with grouped headings for the provider/model picker.
 * Shortcut listeners use {@link DCC_OPEN_MODEL_PICKER_EVENT}.
 */
export function ComposerProviderModelMenu({
	providers,
	selectedProviderId,
	selectedModelId,
	onSelectProvider,
	onSelectModel,
	disabled = false,
}: ComposerProviderModelMenuProps) {
	const [open, setOpen] = useState(false);

	useEffect(() => {
		const openPicker = () => {
			if (!disabled && providers.length > 0) {
				setOpen(true);
			}
		};
		window.addEventListener(DCC_OPEN_MODEL_PICKER_EVENT, openPicker);
		return () =>
			window.removeEventListener(DCC_OPEN_MODEL_PICKER_EVENT, openPicker);
	}, [disabled, providers.length]);

	// Model IDs (e.g. "auto") are only unique *within* a provider — droid and
	// cursor both expose an "auto" model. Always trust the explicit provider
	// selection first; only derive the provider from the model when the
	// selected provider id is stale or missing.
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
			if (owner) {
				return owner;
			}
		}
		return explicit ?? providers[0] ?? null;
	}, [providers, selectedProviderId, selectedModelId]);

	const selectedModel = useMemo(() => {
		if (!selectedModelId || !selectedProvider) {
			return null;
		}
		return (
			selectedProvider.models.find((model) => model.id === selectedModelId) ??
			null
		);
	}, [selectedProvider, selectedModelId]);

	const triggerTitle =
		selectedModel && selectedProvider
			? `${selectedModel.label} — ${selectedProvider.label}`
			: (selectedModel?.label ?? selectedModelId ?? "Select model");

	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger asChild>
				<button
					type="button"
					disabled={disabled || providers.length === 0}
					data-composer-model-picker="true"
					title={triggerTitle}
					aria-label={triggerTitle}
					className={cn(
						`flex min-w-0 max-w-[min(100%,20rem)] items-center gap-1.5 text-muted-foreground ${composerToolbarTriggerClassName}`,
						disabled &&
							"cursor-not-allowed opacity-45 hover:bg-transparent hover:text-muted-foreground",
					)}
				>
					<Sparkles
						className="mt-0.5 size-[13px] shrink-0 self-start"
						strokeWidth={1.8}
					/>
					<div className="min-w-0 flex-1 text-left">
						{selectedModel ? (
							<>
								<div className="truncate text-[13px] font-medium leading-tight text-foreground">
									{selectedModel.label}
								</div>
								{selectedProvider ? (
									<div className="truncate text-[11px] leading-tight text-muted-foreground/90">
										{selectedProvider.label}
									</div>
								) : null}
							</>
						) : (
							<span className="block truncate text-[13px] text-muted-foreground">
								{selectedModelId ?? "Select model"}
							</span>
						)}
					</div>
					<ChevronDown className="size-3 shrink-0 self-center opacity-40" strokeWidth={2} />
				</button>
			</PopoverTrigger>
			<PopoverContent
				side="top"
				align="start"
				sideOffset={4}
				className="w-[min(22rem,calc(100vw-2rem))] max-w-[calc(100vw-2rem)] p-0"
				onCloseAutoFocus={(event) => event.preventDefault()}
			>
				<Command className="rounded-lg border-0 shadow-none">
					<CommandInput placeholder="Search models…" className="h-9" />
					<CommandList>
						<CommandEmpty>No model found.</CommandEmpty>
						{providers.map((provider) => (
							<CommandGroup key={provider.id} heading={provider.label}>
								{provider.models.map((model) => {
									const isActive =
										provider.id === selectedProvider?.id &&
										model.id === selectedModelId;
									const searchBlob = `${provider.label} ${model.label} ${model.id} ${model.description}`;
									return (
										<CommandItem
											key={`${provider.id}-${model.id}`}
											value={searchBlob}
											disabled={disabled}
											onSelect={() => {
												if (provider.id !== selectedProviderId) {
													onSelectProvider(provider.id);
												}
												onSelectModel(model.id);
												setOpen(false);
											}}
											className="[&>svg:last-child]:hidden flex items-center gap-2 font-mono text-[13px] tabular-nums"
										>
											<Sparkles
												className="size-4 shrink-0 text-muted-foreground"
												strokeWidth={1.6}
											/>
											<span className="min-w-0 flex-1 truncate">{model.label}</span>
											{isActive ? (
												<CheckIcon
													className="size-4 shrink-0 text-foreground"
													strokeWidth={2}
													aria-hidden
												/>
											) : (
												<span className="size-4 shrink-0" aria-hidden />
											)}
										</CommandItem>
									);
								})}
							</CommandGroup>
						))}
					</CommandList>
				</Command>
			</PopoverContent>
		</Popover>
	);
}
