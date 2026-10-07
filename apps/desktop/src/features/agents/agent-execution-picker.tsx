import type { ProviderCatalog } from "@dcc/contracts";
import { Check, ChevronDown, ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuSub,
	DropdownMenuSubContent,
	DropdownMenuSubTrigger,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { clampEffort, getEffortDisplay } from "@/features/composer/effort";
import { ProviderIcon } from "@/features/providers/provider-icons";
import { isProviderEnabled } from "@/features/providers/provider-selection.logic";

type Providers = ProviderCatalog["providers"];

/** What an agent runs on. All null: whatever the composer has selected. */
export type AgentExecution = {
	providerId: string | null;
	model: string | null;
	effort: string | null;
};

/**
 * The effort an agent's turn runs with: the stored level, clamped to what the
 * model supports. Null when the agent has none or the model takes none.
 */
export function agentEffortForModel(
	effort: string | null,
	effortLevels: readonly string[] | undefined,
): string | null {
	return effort && effortLevels?.length ? clampEffort(effort, [...effortLevels]) : null;
}

function Selected({ when }: { when: boolean }) {
	return when ? (
		<Check className="ml-auto size-3.5 shrink-0" aria-hidden />
	) : (
		<span className="ml-auto size-3.5 shrink-0" aria-hidden />
	);
}

/**
 * Provider, model and effort in one menu: each level opens the next, and the
 * last click sets all three.
 */
export function AgentExecutionPicker({
	providers,
	value,
	onChange,
}: {
	providers: Providers;
	value: AgentExecution;
	onChange: (value: AgentExecution) => void;
}) {
	const { t } = useTranslation("common");
	const effortLabel = (level: string) =>
		t(`composer.effort.${level}`, { defaultValue: getEffortDisplay(level).label });
	const provider = providers.find((candidate) => candidate.id === value.providerId) ?? null;
	const model = provider?.models.find((candidate) => candidate.id === value.model) ?? null;
	const effort = agentEffortForModel(value.effort, model?.effortLevels);
	const summary = provider
		? [provider.label, model?.label ?? t("agents.editor.modelDefault"), effort && effortLabel(effort)]
				.filter(Boolean)
				.join(" · ")
		: t("agents.editor.providerCurrent");
	const itemClass = "gap-2 text-[13px]";

	return (
		<DropdownMenu>
			<DropdownMenuTrigger
				type="button"
				aria-label={t("agents.editor.runsOnWithSelection", { selection: summary })}
				className="flex h-9 w-full cursor-pointer items-center gap-2 rounded-lg border border-border/80 bg-background px-2.5 text-left text-[13px] text-foreground transition-colors hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
			>
				{provider && <ProviderIcon provider={provider.id} className="size-3.5 shrink-0" />}
				<span className="min-w-0 flex-1 truncate">{summary}</span>
				<ChevronDown className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
			</DropdownMenuTrigger>
			<DropdownMenuContent align="start" className="w-(--radix-dropdown-menu-trigger-width) min-w-56">
				<DropdownMenuItem
					className={itemClass}
					onSelect={() => onChange({ providerId: null, model: null, effort: null })}
				>
					<span className="truncate">{t("agents.editor.providerCurrent")}</span>
					<Selected when={!provider} />
				</DropdownMenuItem>
				<DropdownMenuSeparator />
				{/* Switched-off providers stay out, unless this agent already runs on one. */}
				{providers.filter((candidate) => isProviderEnabled(candidate) || candidate.id === provider?.id).map((candidate) => (
					<DropdownMenuSub key={candidate.id}>
						<DropdownMenuSubTrigger
							className={itemClass}
							disabled={!isProviderEnabled(candidate)}
						>
							<ProviderIcon provider={candidate.id} className="size-3.5 shrink-0" />
							<span className="min-w-0 flex-1 truncate">{candidate.label}</span>
							{candidate.id === provider?.id && <Check className="size-3.5 shrink-0" aria-hidden />}
							<ChevronRight className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
						</DropdownMenuSubTrigger>
						<DropdownMenuSubContent className="min-w-56">
							<DropdownMenuItem
								className={itemClass}
								onSelect={() => onChange({ providerId: candidate.id, model: null, effort: null })}
							>
								<span className="truncate">{t("agents.editor.modelDefault")}</span>
								<Selected when={candidate.id === provider?.id && !model} />
							</DropdownMenuItem>
							{candidate.models.map((choice) => {
								const current = candidate.id === provider?.id && choice.id === model?.id;
								if (choice.effortLevels.length === 0) {
									return (
										<DropdownMenuItem
											key={choice.id}
											className={itemClass}
											onSelect={() =>
												onChange({ providerId: candidate.id, model: choice.id, effort: null })
											}
										>
											<span className="truncate">{choice.label}</span>
											<Selected when={current} />
										</DropdownMenuItem>
									);
								}
								return (
									<DropdownMenuSub key={choice.id}>
										<DropdownMenuSubTrigger className={itemClass}>
											<span className="min-w-0 flex-1 truncate">{choice.label}</span>
											{current && <Check className="size-3.5 shrink-0" aria-hidden />}
											<ChevronRight className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
										</DropdownMenuSubTrigger>
										<DropdownMenuSubContent className="min-w-40">
											<div className="px-1.5 py-1 text-[11px] text-muted-foreground">
												{t("composer.execution.effort")}
											</div>
											{choice.effortLevels.map((level) => (
												<DropdownMenuItem
													key={level}
													className={itemClass}
													onSelect={() =>
														onChange({ providerId: candidate.id, model: choice.id, effort: level })
													}
												>
													<span className="truncate">{effortLabel(level)}</span>
													<Selected when={current && effort === level} />
												</DropdownMenuItem>
											))}
										</DropdownMenuSubContent>
									</DropdownMenuSub>
								);
							})}
						</DropdownMenuSubContent>
					</DropdownMenuSub>
				))}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}
