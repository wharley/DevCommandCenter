import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Bug, Eye, EyeOff, Globe, Terminal, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
	DEBUG_STAGES,
	MAX_DEBUG_EVIDENCE_ITEMS,
	MAX_DEBUG_EVIDENCE_TOTAL_CHARS,
	debugEvidencePreview,
	debugEvidenceTotalChars,
	type DebugEvidenceItem,
	type DebugStage,
} from "@/features/sessions/debug-evidence";

/**
 * Evidence collected by explicit gestures and reviewed by the person before it
 * travels with the next message. Everything the provider will receive is
 * visible here; the tray never adds context on its own.
 */
export type DebugEvidenceController = {
	items: DebugEvidenceItem[];
	stage: DebugStage;
	onRemove: (id: string) => void;
	onClear: () => void;
	onStageChange: (stage: DebugStage) => void;
	/** Called once the turn is accepted so ledger metadata and the tray are settled. */
	onConsumed: (ids: string[]) => void;
};

function formatChars(value: number) {
	return value >= 1_000 ? `${(value / 1_000).toFixed(1)}k` : String(value);
}

export function DebugEvidenceTray({
	controller,
	disabled,
}: {
	controller: DebugEvidenceController;
	disabled: boolean;
}) {
	const { t } = useTranslation("common");
	const [expandedId, setExpandedId] = useState<string | null>(null);
	const { items, stage } = controller;
	if (items.length === 0) return null;
	const totalChars = debugEvidenceTotalChars(items);

	return (
		<div
			className="mb-2 rounded-xl border border-border/45 bg-background/30 px-2.5 py-2"
			data-testid="debug-evidence-tray"
		>
			<div className="mb-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] font-medium text-muted-foreground">
				<span className="flex items-center gap-1.5">
					<Bug className="size-3" strokeWidth={2} />
					{t("composer.evidence.title", { count: items.length })}
				</span>
				<span className="tabular-nums text-muted-foreground/80">
					{t("composer.evidence.budget", {
						used: formatChars(totalChars),
						max: formatChars(MAX_DEBUG_EVIDENCE_TOTAL_CHARS),
						items: items.length,
						maxItems: MAX_DEBUG_EVIDENCE_ITEMS,
					})}
				</span>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					className="ml-auto h-6 gap-1 px-2 text-[11px]"
					disabled={disabled}
					onClick={controller.onClear}
				>
					<Trash2 className="size-3" />
					{t("composer.evidence.clear")}
				</Button>
			</div>
			<div
				className="mb-1.5 flex flex-wrap items-center gap-1"
				role="radiogroup"
				aria-label={t("composer.evidence.stageLabel")}
			>
				<span className="mr-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground/80">
					{t("composer.evidence.stageLabel")}
				</span>
				{DEBUG_STAGES.map((candidate) => {
					const active = candidate === stage;
					return (
						<button
							key={candidate}
							type="button"
							role="radio"
							aria-checked={active}
							disabled={disabled}
							onClick={() => controller.onStageChange(candidate)}
							className={cn(
								"rounded-full border px-2 py-0.5 text-[11px] leading-4 transition-colors",
								active
									? "border-primary/40 bg-primary/10 text-foreground"
									: "border-border/50 text-muted-foreground hover:bg-accent/60 hover:text-foreground",
								disabled && "cursor-not-allowed opacity-60",
							)}
						>
							{t(`composer.evidence.stages.${candidate}`)}
						</button>
					);
				})}
			</div>
			<div className="max-h-48 space-y-1 overflow-y-auto">
				{items.map((item, index) => {
					const expanded = expandedId === item.id;
					const SourceIcon = item.source === "browser" ? Globe : Terminal;
					return (
						<div
							key={item.id}
							className="group rounded-lg bg-muted/35 px-2 py-1.5"
							data-testid="debug-evidence-item"
						>
							<div className="flex min-w-0 items-center gap-1.5">
								<span className="shrink-0 text-[10px] tabular-nums text-muted-foreground/70">
									{index + 1}
								</span>
								<SourceIcon
									className="size-3 shrink-0 text-muted-foreground"
									aria-label={t(`composer.evidence.sources.${item.source}`)}
								/>
								<span
									className="min-w-0 flex-1 truncate text-[12px] text-foreground"
									title={item.label}
								>
									{item.label}
								</span>
								<span
									className={cn(
										"shrink-0 rounded-full border px-1.5 py-px text-[10px] leading-4",
										item.trust === "remote_untrusted"
											? "border-amber-500/40 text-amber-700 dark:text-amber-400"
											: "border-border/60 text-muted-foreground",
									)}
								>
									{t(`composer.evidence.trust.${item.trust}`)}
								</span>
								<span className="shrink-0 text-[10px] tabular-nums text-muted-foreground/80">
									{formatChars(item.chars)}
									{item.truncated ? ` · ${t("composer.evidence.truncated")}` : ""}
								</span>
								<button
									type="button"
									className="shrink-0 rounded-md p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground"
									aria-label={
										expanded
											? t("composer.evidence.hidePreview")
											: t("composer.evidence.preview")
									}
									aria-expanded={expanded}
									onClick={() => setExpandedId(expanded ? null : item.id)}
								>
									{expanded ? <EyeOff className="size-3" /> : <Eye className="size-3" />}
								</button>
								<button
									type="button"
									className="shrink-0 rounded-md p-0.5 text-muted-foreground hover:bg-accent hover:text-destructive"
									aria-label={t("composer.evidence.remove")}
									disabled={disabled}
									onClick={() => controller.onRemove(item.id)}
								>
									<X className="size-3" />
								</button>
							</div>
							{expanded ? (
								<pre className="mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap break-words rounded-md border border-border/40 bg-background/60 p-2 font-mono text-[11px] leading-4 text-muted-foreground">
									{debugEvidencePreview(item)}
								</pre>
							) : null}
						</div>
					);
				})}
			</div>
			<p className="mt-1.5 text-[10px] leading-4 text-muted-foreground/80">
				{t("composer.evidence.hint")}
			</p>
		</div>
	);
}
