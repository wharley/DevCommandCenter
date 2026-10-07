import {
	useEffect,
	useMemo,
	useRef,
	useState,
	type KeyboardEvent as ReactKeyboardEvent,
	type PointerEvent as ReactPointerEvent,
	type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";
import { Check, Plus, RotateCcw, Search, Sparkles, Star, Zap } from "lucide-react";
import type { ProviderCatalog } from "@dcc/contracts";
import { ProviderIcon } from "@/features/providers/provider-icons";
import { getProviderUnhealthyReason } from "@/features/providers/provider-selection.logic";
import { openSettingsSection } from "@/features/settings/settings-navigation";
import { cn } from "@/lib/utils";
import { clampEffort, DEFAULT_EFFORT_LEVEL, getEffortDisplay } from "./effort";
import {
	addModelFavorite,
	getModelFavorites,
	removeModelFavorite,
	saveModelFavorites,
	useModelFavorites,
} from "./model-favorites";
import {
	buildModelPickerRows,
	effortSliderSteps,
	FAVORITES_TAB,
	initialModelPickerTab,
	isModelFavorited,
	MAX_SHORTCUT_ROWS,
	pickerProviders,
	shortcutRowIndex,
	type ModelPickerRow,
} from "./model-picker.logic";

const IS_MAC = typeof navigator !== "undefined" && /mac/i.test(navigator.platform);

/** Brand accent for the open-tab underline; monochrome marks use the foreground. */
const TAB_ACCENT: Record<string, string> = {
	claude_code: "bg-[#D97757]",
	gemini: "bg-[#4285F4]",
	antigravity: "bg-[#4285F4]",
};

export type ModelPickerPanelProps = {
	providers: ProviderCatalog["providers"];
	selectedProviderId: string | null;
	selectedModelId: string | null;
	availableEffortLevels: readonly string[];
	selectedEffortId: string;
	directResponse: boolean;
	disabled?: boolean;
	onSelectProvider: (providerId: string) => void;
	onSelectModel: (modelId: string) => void;
	onSelectEffort: (effortId: string) => void;
	onSelectUltrathink: () => void;
	onSetDirectResponse: (direct: boolean) => void;
	/** Called after a model or favorite is picked. */
	onDone: () => void;
	onEditFavorites: () => void;
	footer?: ReactNode;
};

/**
 * Provider tabs → search → models → effort. One surface, no submenus; the
 * layout follows Synara's picker, the keyboard model follows T3 Code's.
 */
export function ModelPickerPanel({
	providers,
	selectedProviderId,
	selectedModelId,
	availableEffortLevels,
	selectedEffortId,
	directResponse,
	disabled = false,
	onSelectProvider,
	onSelectModel,
	onSelectEffort,
	onSelectUltrathink,
	onSetDirectResponse,
	onDone,
	onEditFavorites,
	footer,
}: ModelPickerPanelProps) {
	const { t } = useTranslation("common");
	const favorites = useModelFavorites();
	const visibleProviders = useMemo(() => pickerProviders(providers), [providers]);
	const [activeTab, setActiveTab] = useState(() =>
		initialModelPickerTab({ providers, favorites, selectedProviderId }),
	);
	const [query, setQuery] = useState("");
	const [highlighted, setHighlighted] = useState(0);
	const searchRef = useRef<HTMLInputElement>(null);
	const listRef = useRef<HTMLDivElement>(null);

	const tabs = useMemo(
		() => [FAVORITES_TAB, ...visibleProviders.map((provider) => provider.id)],
		[visibleProviders],
	);
	const activeProvider = visibleProviders.find((provider) => provider.id === activeTab) ?? null;
	const setupReason = activeProvider ? getProviderUnhealthyReason(activeProvider) : null;
	const rows = useMemo(
		() => buildModelPickerRows({ providers, favorites, activeTab, query }),
		[providers, favorites, activeTab, query],
	);
	const effortLabel = (effort: string) =>
		t(`composer.effort.${effort}`, { defaultValue: getEffortDisplay(effort).label });

	useEffect(() => {
		setHighlighted(0);
	}, [activeTab, query]);

	useEffect(() => {
		searchRef.current?.focus();
	}, [activeTab]);

	useEffect(() => {
		listRef.current
			?.querySelector<HTMLElement>(`[data-row-index="${highlighted}"]`)
			?.scrollIntoView?.({ block: "nearest" });
	}, [highlighted]);

	const pick = (row: ModelPickerRow) => {
		if (disabled) return;
		if (row.providerId !== selectedProviderId) onSelectProvider(row.providerId);
		onSelectModel(row.modelId);
		if (row.favorite) {
			if (row.favorite.effort === "ultrathink") onSelectUltrathink();
			else onSelectEffort(row.favorite.effort ?? DEFAULT_EFFORT_LEVEL);
		}
		onDone();
	};

	// ⌘1…⌘9 picks a visible row while the picker is open, whatever has focus.
	const pickRef = useRef(pick);
	pickRef.current = pick;
	const rowsRef = useRef(rows);
	rowsRef.current = rows;
	useEffect(() => {
		const onKeyDown = (event: KeyboardEvent) => {
			const index = shortcutRowIndex(event, IS_MAC);
			if (index === null) return;
			const row = rowsRef.current[index];
			if (!row) return;
			event.preventDefault();
			event.stopPropagation();
			pickRef.current(row);
		};
		window.addEventListener("keydown", onKeyDown, true);
		return () => window.removeEventListener("keydown", onKeyDown, true);
	}, []);

	const moveTab = (delta: number) => {
		const index = tabs.indexOf(activeTab);
		setActiveTab(tabs[(index + delta + tabs.length) % tabs.length] ?? FAVORITES_TAB);
	};

	const onSearchKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
		// Radix menus/popovers must not steal typing or arrows from the field.
		event.stopPropagation();
		if (event.key === "ArrowDown") {
			event.preventDefault();
			setHighlighted((index) => Math.min(index + 1, Math.max(rows.length - 1, 0)));
		} else if (event.key === "ArrowUp") {
			event.preventDefault();
			setHighlighted((index) => Math.max(index - 1, 0));
		} else if (event.key === "Enter") {
			const row = rows[highlighted];
			if (row) {
				event.preventDefault();
				pick(row);
			}
		} else if ((event.key === "ArrowRight" || event.key === "ArrowLeft") && !query) {
			event.preventDefault();
			moveTab(event.key === "ArrowRight" ? 1 : -1);
		}
	};

	const toggleModelFavorite = (row: ModelPickerRow) => {
		if (row.favorite) {
			removeModelFavorite(row.favorite);
			return;
		}
		if (isModelFavorited(favorites, row.providerId, row.modelId)) {
			saveModelFavorites(
				getModelFavorites().filter(
					(favorite) =>
						favorite.providerId !== row.providerId || favorite.modelId !== row.modelId,
				),
			);
			return;
		}
		const levels =
			visibleProviders
				.find((provider) => provider.id === row.providerId)
				?.models.find((model) => model.id === row.modelId)?.effortLevels ?? [];
		addModelFavorite({
			providerId: row.providerId,
			modelId: row.modelId,
			effort: levels.length
				? selectedEffortId === "ultrathink"
					? "ultrathink"
					: clampEffort(selectedEffortId, [...levels])
				: null,
		});
	};

	const firstOtherIndex = rows.findIndex((row) => row.fromOtherProvider);

	return (
		<div className="flex min-h-0 flex-col" data-model-picker-panel="true">
			<div className="flex shrink-0 items-center gap-0.5 border-b border-border/60 px-1.5">
				<div
					role="tablist"
					aria-label={t("composer.picker.tablist")}
					className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto [scrollbar-width:none]"
				>
					{tabs.map((tab) => {
						const provider = visibleProviders.find((entry) => entry.id === tab);
						const reason = provider ? getProviderUnhealthyReason(provider) : null;
						const label = provider
							? reason
								? t("composer.picker.providerUnavailable", { provider: provider.label })
								: provider.label
							: t("composer.favorites.title");
						const selected = tab === activeTab;
						return (
							<button
								key={tab}
								type="button"
								role="tab"
								aria-selected={selected}
								aria-label={label}
								title={label}
								onClick={() => setActiveTab(tab)}
								className={cn(
									"relative inline-flex h-10 min-w-9 shrink-0 items-center justify-center rounded-lg px-2 text-muted-foreground transition-colors hover:bg-accent/70 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
									selected && "text-foreground",
								)}
							>
								{provider ? (
									<ProviderIcon
										provider={provider.id}
										className={cn("size-[17px]", reason && "opacity-40")}
									/>
								) : (
									<Star
										className="size-4"
										fill={selected ? "currentColor" : "none"}
										strokeWidth={1.75}
									/>
								)}
								{selected ? (
									<span
										aria-hidden
										className={cn(
											"absolute inset-x-2 bottom-0 h-0.5 rounded-full",
											(provider && TAB_ACCENT[provider.id]) ?? "bg-foreground",
										)}
									/>
								) : null}
							</button>
						);
					})}
				</div>
				<button
					type="button"
					title={t("composer.picker.manageProviders")}
					aria-label={t("composer.picker.manageProviders")}
					onClick={() => {
						onDone();
						openSettingsSection("model");
					}}
					className="inline-flex size-8 shrink-0 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-accent/70 hover:text-foreground"
				>
					<Plus className="size-4" strokeWidth={1.75} />
				</button>
			</div>

			<label className="flex h-10 shrink-0 items-center gap-2 border-b border-border/60 px-3">
				<Search className="size-3.5 shrink-0 text-muted-foreground/60" aria-hidden />
				<input
					ref={searchRef}
					value={query}
					onChange={(event) => setQuery(event.currentTarget.value)}
					onKeyDown={onSearchKeyDown}
					placeholder={t("composer.model.search")}
					aria-label={t("composer.model.search")}
					aria-controls="dcc-model-picker-list"
					aria-activedescendant={rows[highlighted] ? `dcc-model-row-${highlighted}` : undefined}
					role="combobox"
					aria-expanded="true"
					className="min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-muted-foreground/60"
				/>
			</label>

			<div
				ref={listRef}
				id="dcc-model-picker-list"
				role="listbox"
				aria-label={t("composer.model.select")}
				className="max-h-[min(15rem,38vh)] min-h-24 overflow-y-auto overscroll-contain p-1"
			>
				{setupReason && !query ? (
					<div className="flex flex-col items-start gap-2 px-2.5 py-3">
						<p className="text-[12px] font-medium">
							{t("composer.picker.providerUnavailable", { provider: activeProvider?.label })}
						</p>
						<p className="text-[11px] leading-relaxed text-muted-foreground">{setupReason}</p>
						<button
							type="button"
							className="rounded-md border border-border/70 px-2 py-1 text-[11px] font-medium hover:bg-accent"
							onClick={() => {
								onDone();
								openSettingsSection("model");
							}}
						>
							{t("composer.picker.openSetup")}
						</button>
					</div>
				) : rows.length === 0 ? (
					<p className="px-2.5 py-4 text-center text-[12px] leading-relaxed text-muted-foreground">
						{activeTab === FAVORITES_TAB && !query
							? t("composer.picker.favoritesEmpty")
							: t("composer.model.empty")}
					</p>
				) : (
					rows.map((row, index) => {
						const isActive =
							row.providerId === selectedProviderId &&
							row.modelId === selectedModelId &&
							(!row.favorite ||
								row.favorite.effort === null ||
								row.favorite.effort === selectedEffortId);
						const starred = row.favorite
							? true
							: isModelFavorited(favorites, row.providerId, row.modelId);
						return (
							<div key={row.key}>
								{index === firstOtherIndex ? (
									<p className="px-2.5 pb-1 pt-2 text-[10px] font-medium uppercase tracking-[0.06em] text-muted-foreground/70">
										{t("composer.picker.otherProviders")}
									</p>
								) : null}
								<div
									id={`dcc-model-row-${index}`}
									data-row-index={index}
									role="option"
									aria-selected={isActive}
									aria-disabled={disabled || undefined}
									title={row.description || undefined}
									onMouseMove={() => setHighlighted(index)}
									onClick={() => pick(row)}
									className={cn(
										"group flex h-8 cursor-default select-none items-center gap-2 rounded-lg pl-2.5 pr-1 text-[13px]",
										index === highlighted && "bg-accent",
										disabled && "pointer-events-none opacity-50",
									)}
								>
									{row.favorite || row.fromOtherProvider ? (
										<ProviderIcon provider={row.providerId} className="size-3.5 shrink-0" />
									) : null}
									<span className={cn("min-w-0 flex-1 truncate", isActive && "font-medium")}>
										{row.label}
									</span>
									{row.favorite ? (
										<span className="shrink-0 text-[11px] text-muted-foreground">
											{row.favorite.effort === null
												? t("composer.favorites.managed")
												: effortLabel(row.favorite.effort)}
										</span>
									) : null}
									{isActive ? (
										<Check className="size-3.5 shrink-0 text-foreground" strokeWidth={2.25} aria-hidden />
									) : null}
									{index < MAX_SHORTCUT_ROWS ? (
										<kbd className="hidden shrink-0 rounded-md bg-muted px-1.5 py-px font-sans text-[10px] font-medium tabular-nums text-muted-foreground sm:inline-block">
											{IS_MAC ? `⌘${index + 1}` : `Ctrl ${index + 1}`}
										</kbd>
									) : null}
									<button
										type="button"
										tabIndex={-1}
										aria-label={t(
											starred ? "composer.picker.favoriteRemove" : "composer.picker.favoriteAdd",
											{ model: row.label },
										)}
										aria-pressed={starred}
										onClick={(event) => {
											event.stopPropagation();
											toggleModelFavorite(row);
										}}
										className={cn(
											"inline-flex size-6 shrink-0 items-center justify-center rounded-md transition-colors hover:bg-background/70",
											starred
												? "text-amber-500"
												: "text-muted-foreground/35 hover:text-foreground group-hover:text-muted-foreground/70",
										)}
									>
										<Star className="size-3.5" fill={starred ? "currentColor" : "none"} />
									</button>
								</div>
							</div>
						);
					})
				)}
				{activeTab === FAVORITES_TAB && !query ? (
					<button
						type="button"
						onClick={onEditFavorites}
						className="mt-0.5 w-full rounded-lg px-2.5 py-1.5 text-left text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground"
					>
						{t("composer.favorites.edit")}
					</button>
				) : null}
			</div>

			<EffortSection
				levels={availableEffortLevels}
				selectedEffortId={selectedEffortId}
				directResponse={directResponse}
				disabled={disabled}
				onSelectEffort={onSelectEffort}
				onSelectUltrathink={onSelectUltrathink}
				onSetDirectResponse={onSetDirectResponse}
			/>
			{footer}
		</div>
	);
}

function EffortSection({
	levels,
	selectedEffortId,
	directResponse,
	disabled,
	onSelectEffort,
	onSelectUltrathink,
	onSetDirectResponse,
}: {
	levels: readonly string[];
	selectedEffortId: string;
	directResponse: boolean;
	disabled: boolean;
	onSelectEffort: (effortId: string) => void;
	onSelectUltrathink: () => void;
	onSetDirectResponse: (direct: boolean) => void;
}) {
	const { t } = useTranslation("common");
	const steps = effortSliderSteps(levels);
	const label = steps.length
		? t(`composer.effort.${selectedEffortId}`, {
				defaultValue: getEffortDisplay(selectedEffortId).label,
			})
		: t("composer.picker.managedEffort");
	const directLabel = t(
		directResponse ? "composer.picker.directOn" : "composer.picker.directOff",
	);

	return (
		<div className="shrink-0 border-t border-border/60 px-3 pb-3 pt-2">
			<div className="grid grid-cols-[1.75rem_minmax(0,1fr)_1.75rem] items-center">
				<button
					type="button"
					aria-pressed={directResponse}
					aria-label={directLabel}
					title={`${directLabel} — ${t("composer.execution.response.directHint")}`}
					disabled={disabled}
					onClick={() => onSetDirectResponse(!directResponse)}
					className={cn(
						"inline-flex size-7 items-center justify-center rounded-md transition-colors hover:bg-accent disabled:opacity-50",
						directResponse ? "text-amber-500" : "text-muted-foreground/70 hover:text-foreground",
					)}
				>
					<Zap className="size-3.5" fill={directResponse ? "currentColor" : "none"} />
				</button>
				<p
					className={cn(
						"truncate text-center text-[13px] font-semibold",
						steps.length ? "text-info" : "text-muted-foreground",
					)}
					aria-live="polite"
				>
					{label}
				</p>
				{steps.length ? (
					<button
						type="button"
						aria-label={t("composer.execution.resetEffort")}
						title={t("composer.execution.resetEffort")}
						disabled={disabled}
						onClick={() => onSelectEffort(clampEffort(DEFAULT_EFFORT_LEVEL, [...levels]))}
						className="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground/70 transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50"
					>
						<RotateCcw className="size-3.5" />
					</button>
				) : (
					<span />
				)}
			</div>
			{steps.length ? (
				<EffortSlider
					steps={steps}
					value={selectedEffortId}
					valueText={label}
					disabled={disabled}
					onChange={(step) =>
						step === "ultrathink" ? onSelectUltrathink() : onSelectEffort(step)
					}
				/>
			) : null}
		</div>
	);
}

/** Pill track with a stop per effort level; the thumb snaps to the nearest stop. */
function EffortSlider({
	steps,
	value,
	valueText,
	disabled,
	onChange,
}: {
	steps: string[];
	value: string;
	valueText: string;
	disabled: boolean;
	onChange: (step: string) => void;
}) {
	const { t } = useTranslation("common");
	const trackRef = useRef<HTMLDivElement>(null);
	const draggingRef = useRef(false);
	const last = steps.length - 1;
	const index = Math.max(0, steps.indexOf(value));
	// Keep the thumb (28px) fully inside the track at both ends.
	const position = (step: number) =>
		`calc(18px + (100% - 36px) * ${last === 0 ? 0 : step / last})`;

	const commit = (next: number) => {
		const clamped = Math.min(last, Math.max(0, next));
		const step = steps[clamped];
		if (step && clamped !== index) onChange(step);
	};
	const stepFromPointer = (event: ReactPointerEvent<HTMLDivElement>) => {
		const rect = trackRef.current?.getBoundingClientRect();
		if (!rect || rect.width <= 36) return index;
		const ratio = (event.clientX - rect.left - 18) / (rect.width - 36);
		return Math.round(Math.min(1, Math.max(0, ratio)) * last);
	};

	return (
		<div
			ref={trackRef}
			role="slider"
			tabIndex={disabled ? -1 : 0}
			aria-label={t("composer.execution.effort")}
			aria-valuemin={0}
			aria-valuemax={last}
			aria-valuenow={index}
			aria-valuetext={valueText}
			aria-disabled={disabled || undefined}
			onKeyDown={(event) => {
				const moves: Record<string, number> = {
					ArrowRight: index + 1,
					ArrowUp: index + 1,
					ArrowLeft: index - 1,
					ArrowDown: index - 1,
					Home: 0,
					End: last,
				};
				if (event.key in moves) {
					event.preventDefault();
					event.stopPropagation();
					if (!disabled) commit(moves[event.key]!);
				}
			}}
			onPointerDown={(event) => {
				if (disabled) return;
				draggingRef.current = true;
				event.currentTarget.setPointerCapture?.(event.pointerId);
				commit(stepFromPointer(event));
			}}
			onPointerMove={(event) => {
				if (draggingRef.current) commit(stepFromPointer(event));
			}}
			onPointerUp={() => {
				draggingRef.current = false;
			}}
			className={cn(
				"relative mt-2 h-9 touch-none select-none rounded-full bg-muted outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
				disabled ? "opacity-50" : "cursor-pointer",
			)}
		>
			<div
				aria-hidden
				className="absolute inset-y-0 left-0 rounded-full bg-info transition-[width] duration-150 ease-out"
				style={{ width: `calc(${position(index)} + 14px)` }}
			/>
			{steps.map((step, stepIndex) => (
				<span
					key={step}
					aria-hidden
					className="absolute top-1/2 -translate-x-1/2 -translate-y-1/2"
					style={{ left: position(stepIndex) }}
				>
					{step === "ultrathink" ? (
						<Sparkles
							className={cn(
								"size-3",
								stepIndex <= index ? "text-white/85" : "text-muted-foreground/60",
							)}
						/>
					) : (
						<span
							className={cn(
								"block size-1 rounded-full",
								stepIndex <= index ? "bg-white/75" : "bg-muted-foreground/45",
							)}
						/>
					)}
				</span>
			))}
			<span
				aria-hidden
				className="absolute top-1/2 size-7 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white shadow-[0_1px_3px_rgba(0,0,0,0.25),0_0_0_0.5px_rgba(0,0,0,0.08)] transition-[left] duration-150 ease-out"
				style={{ left: position(index) }}
			/>
		</div>
	);
}

