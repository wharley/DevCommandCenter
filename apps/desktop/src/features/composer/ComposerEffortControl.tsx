import { useState } from "react";
import { useTranslation } from "react-i18next";
import { RotateCcw } from "lucide-react";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import { EffortBrainIcon } from "./EffortBrainIcon";
import { composerToolbarTriggerClassName } from "./WorkspaceComposer.logic";
import {
	clampEffort,
	DEFAULT_EFFORT_LEVEL,
	getEffortDisplay,
} from "./effort";

type ComposerEffortControlProps = {
	modelLabel: string;
	availableEffortLevels: readonly string[];
	selectedEffortId: string;
	disabled?: boolean;
	onSelectEffort: (effortId: string) => void;
	onSelectUltrathink: () => void;
};

export function ComposerEffortControl({
	modelLabel,
	availableEffortLevels,
	selectedEffortId,
	disabled = false,
	onSelectEffort,
	onSelectUltrathink,
}: ComposerEffortControlProps) {
	const { t } = useTranslation("common");
	const [open, setOpen] = useState(false);
	if (availableEffortLevels.length === 0) return null;

	const selectedLevel = selectedEffortId === "ultrathink"
		? availableEffortLevels.length - 1
		: Math.max(0, availableEffortLevels.indexOf(selectedEffortId));
	const selectedLabel = selectedEffortId === "ultrathink"
		? t("composer.effort.ultrathink")
		: t(`composer.effort.${selectedEffortId}`, {
			defaultValue: getEffortDisplay(selectedEffortId).label,
		});
	const currentModelEffort = availableEffortLevels[selectedLevel] ?? availableEffortLevels[0];
	const currentEffortLabel = t(`composer.effort.${currentModelEffort}`, {
		defaultValue: getEffortDisplay(currentModelEffort).label,
	});

	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger asChild>
				<button
					type="button"
					disabled={disabled}
					aria-label={`${t("composer.execution.effort")}: ${selectedLabel}`}
					aria-expanded={open}
					className={cn(
						`flex h-7 max-w-28 items-center gap-1 ${composerToolbarTriggerClassName}`,
						"text-muted-foreground",
						disabled && "cursor-not-allowed opacity-45 hover:bg-transparent",
					)}
				>
					<EffortBrainIcon level={getEffortDisplay(selectedEffortId).icon} />
					<span className="dcc-composer-effort-summary truncate text-[12px] font-medium leading-4 text-foreground">
						{selectedLabel}
					</span>
				</button>
			</PopoverTrigger>
			<PopoverContent side="top" align="end" className="w-64 gap-3 p-3">
				<div className="flex items-start justify-between gap-2">
					<div className="min-w-0">
						<div className="text-[13px] font-semibold leading-4">{selectedLabel}</div>
						<div className="truncate text-[11px] text-muted-foreground">{modelLabel}</div>
					</div>
					<button
						type="button"
						disabled={disabled}
						className="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
						aria-label={t("composer.execution.resetEffort")}
						title={t("composer.execution.resetEffort")}
						onClick={() => onSelectEffort(clampEffort(DEFAULT_EFFORT_LEVEL, [...availableEffortLevels]))}
					>
						<RotateCcw className="size-3.5" />
					</button>
				</div>

				<div className="px-1 pt-1">
					<input
						type="range"
						disabled={disabled}
						min={0}
						max={availableEffortLevels.length - 1}
						step={1}
						value={selectedLevel}
						aria-label={t("composer.execution.effort")}
						aria-valuetext={currentEffortLabel}
						className="h-4 w-full cursor-pointer accent-primary"
						onChange={(event) => {
							const level = availableEffortLevels[Number(event.currentTarget.value)];
							if (level) onSelectEffort(level);
						}}
					/>
					<div
						className="mt-1 grid text-center text-[10px] leading-3 text-muted-foreground"
						style={{ gridTemplateColumns: `repeat(${availableEffortLevels.length}, minmax(0, 1fr))` }}
					>
						{availableEffortLevels.map((level, index) => (
							<span key={level} className={cn(index === selectedLevel && "font-semibold text-foreground")}>
								{t(`composer.effort.${level}`, {
									defaultValue: getEffortDisplay(level).label,
								})}
							</span>
						))}
					</div>
				</div>

				<button
					type="button"
					disabled={disabled}
					aria-pressed={selectedEffortId === "ultrathink"}
					className={cn(
						"flex h-8 w-full items-center justify-between rounded-md px-2 text-left text-xs hover:bg-accent",
						selectedEffortId === "ultrathink" && "bg-accent font-medium",
					)}
					onClick={onSelectUltrathink}
				>
					<span>{t("composer.effort.ultrathink")}</span>
					<span className="text-[10px] text-muted-foreground">{t("composer.execution.ultrathinkHint")}</span>
				</button>
			</PopoverContent>
		</Popover>
	);
}
