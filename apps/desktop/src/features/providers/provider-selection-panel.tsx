import { Check, ChevronDown, Sparkles } from "lucide-react";
import type { ProviderCatalog } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import {
	getProviderChips,
	getRecommendedProviderModel,
	summarizeProviderHealth,
} from "./provider-display";

type ProviderSelectionPanelProps = {
	title?: string;
	description?: string;
	providers: ProviderCatalog["providers"];
	selectedProviderId: string | null;
	onSelectProvider: (providerId: string) => void;
	selectedModelId: string | null;
	onSelectModel: (modelId: string) => void;
	compact?: boolean;
	className?: string;
};

function ProviderChipRow({
	provider,
	className,
}: {
	provider: ProviderCatalog["providers"][number];
	className?: string;
}) {
	const chips = getProviderChips(provider);
	return (
		<div className={cn("flex flex-wrap gap-1.5", className)}>
			{chips.map((chip) => (
				<Badge key={`${provider.id}-${chip.label}`} variant={chip.variant}>
					{chip.label}
				</Badge>
			))}
		</div>
	);
}

function ProviderModelCards({
	provider,
	selectedModelId,
	onSelectModel,
}: {
	provider: ProviderCatalog["providers"][number];
	selectedModelId: string | null;
	onSelectModel: (modelId: string) => void;
}) {
	if (provider.models.length === 0) {
		return (
			<div className="rounded-lg border border-dashed border-border/60 bg-background/60 p-3 text-[12px] text-muted-foreground">
				No models registered for this provider yet.
			</div>
		);
	}

	return (
		<div className="grid gap-2">
			{provider.models.map((model) => (
				<button
					key={`${provider.id}-${model.id}`}
					type="button"
					onClick={() => onSelectModel(model.id)}
					className={cn(
						"flex w-full items-start justify-between gap-3 rounded-lg border px-3 py-2 text-left transition-colors",
						model.id === selectedModelId
							? "border-foreground/20 bg-accent/70"
							: model.recommended
								? "border-border/80 bg-background hover:bg-accent/30"
								: "border-border/60 bg-muted/10 hover:bg-accent/20",
					)}
				>
					<div className="min-w-0">
						<div className="flex items-center gap-2">
							<span className="truncate text-[13px] font-medium text-foreground">
								{model.label}
							</span>
							{model.recommended ? (
								<Badge variant="success">default</Badge>
							) : null}
							{model.id === selectedModelId ? (
								<Badge variant="outline">selected</Badge>
							) : null}
						</div>
						<p className="mt-0.5 text-[11px] leading-snug text-muted-foreground">
							{model.description}
						</p>
					</div>
					<Badge variant="outline" className="shrink-0">
						{model.id}
					</Badge>
				</button>
			))}
		</div>
	);
}

function CompactProviderPicker({
	providers,
	selectedProvider,
	selectedModel,
	onSelectProvider,
}: {
	providers: ProviderCatalog["providers"];
	selectedProvider: ProviderCatalog["providers"][number] | null;
	selectedModel: ProviderCatalog["providers"][number]["models"][number] | null;
	onSelectProvider: (providerId: string) => void;
}) {
	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button
					type="button"
					variant="outline"
					size="sm"
					className="min-w-[180px] justify-between gap-2 rounded-[10px] px-3 text-left"
					disabled={providers.length === 0}
				>
					<div className="min-w-0">
						<div className="truncate text-[12px] font-medium">
							{selectedProvider?.label ?? "No providers"}
						</div>
						<div className="truncate text-[10px] text-muted-foreground">
							{selectedModel?.label ??
								getRecommendedProviderModel(selectedProvider ?? null)?.label ??
								(selectedProvider ? selectedProvider.description : "Waiting for runtime catalog")}
						</div>
					</div>
					<ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end" sideOffset={8} className="min-w-[340px] p-1.5">
				<DropdownMenuLabel className="px-2 pb-1 pt-0.5 text-[10px] uppercase tracking-[0.08em] text-muted-foreground">
					Provider catalog
				</DropdownMenuLabel>
				<DropdownMenuSeparator />
				{providers.length === 0 ? (
					<DropdownMenuItem disabled className="px-2 py-2 text-[12px]">
						No providers configured
					</DropdownMenuItem>
				) : null}
				{providers.map((provider) => {
					const health = summarizeProviderHealth(provider.health);
					const isSelected = provider.id === (selectedProvider?.id ?? null);
					return (
						<DropdownMenuItem
							key={provider.id}
							className={cn(
								"flex flex-col items-stretch gap-2 rounded-lg px-2 py-2.5",
								isSelected && "bg-accent/70 text-accent-foreground",
							)}
							onSelect={(event) => {
								event.preventDefault();
								onSelectProvider(provider.id);
							}}
						>
							<div className="flex items-start justify-between gap-3">
								<div className="min-w-0">
									<div className="flex items-center gap-2">
										<span className="truncate text-[13px] font-medium text-foreground">
											{provider.label}
										</span>
										{isSelected ? (
											<Check className="size-3.5 text-foreground" aria-hidden />
										) : null}
									</div>
									<p className="mt-0.5 text-[11px] leading-snug text-muted-foreground">
										{provider.description}
									</p>
								</div>
								<div className="flex shrink-0 flex-col items-end gap-1">
									<Badge variant={provider.stable ? "success" : "outline"}>
										{provider.stable ? "stable" : "experimental"}
									</Badge>
									<Badge variant={health.variant}>{health.label}</Badge>
								</div>
							</div>
							<ProviderChipRow provider={provider} />
						</DropdownMenuItem>
					);
				})}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

export function ProviderSelectionPanel({
	title = "Provider",
	description = "Choose the engine for new sessions.",
	providers,
	selectedProviderId,
	onSelectProvider,
	selectedModelId,
	onSelectModel,
	compact = false,
	className,
}: ProviderSelectionPanelProps) {
	const selectedProvider =
		providers.find((provider) => provider.id === selectedProviderId) ??
		providers[0] ??
		null;
	const selectedModel =
		selectedProvider?.models.find((model) => model.id === selectedModelId) ?? null;

	if (compact) {
		return (
			<div className={cn("space-y-2", className)}>
				<div className="flex items-center justify-between gap-3">
					<div className="min-w-0">
						<p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
							{title}
						</p>
						<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
							{description}
						</p>
					</div>
					<CompactProviderPicker
						providers={providers}
						selectedProvider={selectedProvider}
						selectedModel={selectedModel}
						onSelectProvider={onSelectProvider}
					/>
				</div>
				{selectedProvider ? (
					<div className="rounded-xl border border-border/60 bg-muted/15 p-3">
						<div className="flex items-start justify-between gap-3">
							<div className="min-w-0">
								<div className="flex items-center gap-2">
									<Sparkles className="size-4 shrink-0 text-muted-foreground" aria-hidden />
									<h4 className="truncate text-[14px] font-medium text-foreground">
										{selectedProvider.label}
									</h4>
								</div>
								<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
									{selectedProvider.description}
								</p>
							</div>
							<Badge variant="outline" className="shrink-0">
								{selectedProvider.id}
							</Badge>
						</div>
						<div className="mt-3 flex flex-wrap gap-1.5">
							{getProviderChips(selectedProvider).map((chip) => (
								<Badge key={`${selectedProvider.id}-${chip.label}`} variant={chip.variant}>
									{chip.label}
								</Badge>
							))}
						</div>
					</div>
				) : (
					<div className="rounded-xl border border-dashed border-border/60 bg-muted/10 p-4 text-[12px] text-muted-foreground">
						No providers available yet.
					</div>
				)}
			</div>
		);
	}

	return (
		<div
			className={cn(
				"overflow-hidden rounded-2xl border border-border/60 bg-background",
				className,
			)}
		>
			<div className="grid min-h-[300px] grid-cols-[240px_minmax(0,1fr)]">
				<aside className="flex flex-col border-r border-border/60 bg-sidebar/55">
					<div className="px-4 py-3">
						<p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
							{title}
						</p>
						<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
							{description}
						</p>
					</div>
					<div className="flex-1 overflow-y-auto px-2 pb-2">
						{providers.length === 0 ? (
							<div className="rounded-lg border border-dashed border-border/60 bg-background/70 px-3 py-4 text-[12px] text-muted-foreground">
								No providers configured
							</div>
						) : null}
						{providers.map((provider) => {
							const isSelected = provider.id === selectedProvider?.id;
							return (
								<button
									key={provider.id}
									type="button"
									onClick={() => onSelectProvider(provider.id)}
									className={cn(
										"flex w-full items-start gap-3 rounded-xl px-3 py-2.5 text-left transition-colors",
										isSelected
											? "bg-accent/80 text-foreground"
											: "text-muted-foreground hover:bg-accent/40 hover:text-foreground",
									)}
								>
									<div className="min-w-0 flex-1">
										<div className="flex items-center gap-2">
											<span className="truncate text-[13px] font-medium">
												{provider.label}
											</span>
											{isSelected ? (
												<Check className="size-3.5 shrink-0 text-foreground" aria-hidden />
											) : null}
										</div>
										<p className="mt-0.5 text-[11px] leading-snug text-muted-foreground/90">
											{provider.models.length > 0
												? `${provider.models.length} models`
												: "No models"}
										</p>
									</div>
									<Badge variant={provider.stable ? "success" : "outline"}>
										{provider.stable ? "stable" : "experimental"}
									</Badge>
								</button>
							);
						})}
					</div>
				</aside>

				<section className="flex min-w-0 flex-col overflow-hidden">
					{selectedProvider ? (
						<>
							<div className="flex items-start justify-between gap-4 border-b border-border/60 px-5 py-4">
								<div className="min-w-0">
									<div className="flex items-center gap-2">
										<Sparkles className="size-4 shrink-0 text-muted-foreground" aria-hidden />
										<h4 className="truncate text-[15px] font-medium text-foreground">
											{selectedProvider.label}
										</h4>
									</div>
									<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
										{selectedProvider.description}
									</p>
								</div>
								<div className="flex shrink-0 flex-col items-end gap-1">
									<Badge variant="outline">{selectedProvider.id}</Badge>
									<Badge variant={selectedProvider.stable ? "success" : "outline"}>
										{selectedProvider.stable ? "stable" : "experimental"}
									</Badge>
								</div>
							</div>

							<div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
								<div className="space-y-4">
									<ProviderChipRow provider={selectedProvider} />
									<div className="space-y-2">
										<p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
											Models
										</p>
										<ProviderModelCards
											provider={selectedProvider}
											selectedModelId={selectedModelId}
											onSelectModel={onSelectModel}
										/>
									</div>
								</div>
							</div>
						</>
					) : (
						<div className="flex flex-1 items-center justify-center px-6 py-8 text-[12px] text-muted-foreground">
							No provider selected.
						</div>
					)}
				</section>
			</div>
		</div>
	);
}
