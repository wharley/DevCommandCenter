import { ChevronRight, CornerUpRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ProviderCatalog } from "@dcc/contracts";
import {
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSub,
	DropdownMenuSubContent,
	DropdownMenuSubTrigger,
} from "@/components/ui/dropdown-menu";
import { ProviderIcon } from "@/features/providers/provider-icons";

export type DelegationTargetSelection = {
	providerId: string;
	modelId: string | null;
};

export function recommendedDelegationModel(
	target: ProviderCatalog["providers"][number],
) {
	return target.models.find((model) => model.recommended) ?? target.models[0] ?? null;
}

/**
 * Reusable split rows for delegation menus. The wide part keeps the fast path:
 * one click starts the recommended model. The chevron exposes the provider's
 * full model catalog without introducing a configuration modal.
 */
export function DelegationTargetItems({
	targets,
	disabled = false,
	onSelect,
}: {
	targets: ProviderCatalog["providers"];
	disabled?: boolean;
	onSelect: (selection: DelegationTargetSelection) => void;
}) {
	const { t } = useTranslation("common");

	return targets.map((target) => {
		const recommendedModel = recommendedDelegationModel(target);
		const canChooseModel = target.models.length > 1;
		return (
			<DropdownMenuSub key={target.id}>
				<div className="flex min-w-0 items-stretch rounded-md focus-within:bg-accent">
					<DropdownMenuItem
						disabled={disabled || !recommendedModel}
						className="min-w-0 flex-1 rounded-r-none pr-1"
						onSelect={() =>
							onSelect({
								providerId: target.id,
								modelId: recommendedModel?.id ?? null,
							})
						}
					>
						<ProviderIcon provider={target.id} className="size-4 shrink-0" />
						<div className="min-w-0 flex-1">
							<div className="truncate text-[13px] font-medium">{target.label}</div>
							<div className="truncate text-[11px] text-muted-foreground">
								{recommendedModel?.label ?? t("composer.delegate.noModels")}
							</div>
						</div>
						<CornerUpRight className="size-3.5 shrink-0 opacity-40" strokeWidth={2} />
					</DropdownMenuItem>
					{canChooseModel ? (
						<DropdownMenuSubTrigger
							disabled={disabled}
							aria-label={t("composer.delegate.chooseModel", {
								provider: target.label,
							})}
							className="w-7 shrink-0 justify-center rounded-l-none border-l border-border/50 px-0"
						>
							<ChevronRight className="size-3.5 opacity-60" strokeWidth={2} />
						</DropdownMenuSubTrigger>
					) : null}
				</div>
				{canChooseModel ? (
					<DropdownMenuSubContent className="w-64">
						<DropdownMenuLabel>{target.label}</DropdownMenuLabel>
						{target.models.map((model) => (
							<DropdownMenuItem
								key={model.id}
								disabled={disabled}
								className="flex items-center justify-between gap-3"
								onSelect={() =>
									onSelect({ providerId: target.id, modelId: model.id })
								}
							>
								<span className="min-w-0 truncate">{model.label}</span>
								{model.recommended ? (
									<span className="shrink-0 text-[10px] text-muted-foreground">
										{t("composer.delegate.recommended")}
									</span>
								) : null}
							</DropdownMenuItem>
						))}
					</DropdownMenuSubContent>
				) : null}
			</DropdownMenuSub>
		);
	});
}
