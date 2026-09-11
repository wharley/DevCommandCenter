import { useId, useState } from "react";
import { Reorder, useDragControls } from "motion/react";
import { ArrowDown, ArrowUp, GripVertical, Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ProviderCatalog } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import {
	Dialog, DialogClose, DialogContent, DialogDescription,
	DialogFooter, DialogHeader, DialogTitle,
} from "@/components/ui/dialog";
import { ProviderIcon } from "@/features/providers/provider-icons";
import { isProviderEnabled } from "@/features/providers/provider-selection.logic";
import { clampEffort, getEffortDisplay } from "./effort";
import {
	addModelFavorite, modelFavoriteKey, removeModelFavorite,
	resolveModelFavorite, saveModelFavorites, useModelFavorites,
	type ModelFavorite,
} from "./model-favorites";

const selectClass = "h-9 w-full min-w-0 rounded-md border border-input bg-background px-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50";
const iconButtonClass = "flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-30";

function FavoriteRow({ favorite, providers, favorites, index }: {
	favorite: ModelFavorite;
	providers: ProviderCatalog["providers"];
	favorites: ModelFavorite[];
	index: number;
}) {
	const { t } = useTranslation("common");
	const controls = useDragControls();
	const { provider, model, available } = resolveModelFavorite(favorite, providers);
	const key = modelFavoriteKey(favorite);
	const label = model?.label ?? favorite.modelId;
	const effortOptions = model?.effortLevels.length
		? [...model.effortLevels, "ultrathink"] : [];
	if (favorite.effort !== null && !effortOptions.includes(favorite.effort)) {
		effortOptions.push(favorite.effort);
	}
	const move = (offset: number) => {
		const target = index + offset;
		if (target < 0 || target >= favorites.length) return;
		const next = [...favorites];
		[next[index], next[target]] = [next[target], next[index]];
		saveModelFavorites(next);
	};
	return (
		<Reorder.Item
			value={key}
			dragListener={false}
			dragControls={controls}
			className="relative grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2 rounded-lg border border-border bg-popover p-2 sm:flex"
		>
			<button type="button" className={`${iconButtonClass} row-span-2 touch-none cursor-grab active:cursor-grabbing`}
				aria-label={t("composer.favorites.reorder", { model: label })}
				title={t("composer.favorites.reorderHint")}
				onPointerDown={(event) => controls.start(event)}
				onKeyDown={(event) => {
					if (event.key === "ArrowUp" || event.key === "ArrowDown") {
						event.preventDefault();
						move(event.key === "ArrowUp" ? -1 : 1);
					}
				}}
			><GripVertical className="size-4" /></button>
			<div className="col-span-2 flex min-w-0 flex-1 items-center gap-2">
				<ProviderIcon provider={favorite.providerId} className="size-4 shrink-0" />
				<div className="min-w-0 flex-1">
					<div className="truncate text-sm font-medium" title={label}>{label}</div>
					<div className="truncate text-[11px] text-muted-foreground">
						{provider?.label ?? favorite.providerId}
						{!available ? ` · ${t("composer.favorites.unavailable")}` : ""}
					</div>
				</div>
			</div>
			<select className={`${selectClass} col-start-2 shrink-0 text-xs sm:w-28`}
				aria-label={t("composer.favorites.effortFor", { model: label })}
				value={favorite.effort ?? ""}
				disabled={!model}
				onChange={(event) => saveModelFavorites(favorites.map((entry, entryIndex) =>
					entryIndex === index ? { ...entry, effort: event.target.value || null } : entry,
				))}
			>
				{(!model?.effortLevels.length || favorite.effort === null) &&
					<option value="">{t("composer.favorites.managed")}</option>}
				{effortOptions.map((effort) => (
					<option key={effort} value={effort}
						disabled={favorites.some((entry, entryIndex) => entryIndex !== index &&
							modelFavoriteKey(entry) === modelFavoriteKey({ ...favorite, effort }))}
					>{t(`composer.effort.${effort}`, { defaultValue: getEffortDisplay(effort).label })}</option>
				))}
			</select>
			<div className="flex shrink-0 items-center gap-1">
			<div className="flex sm:flex-col">
				<button type="button" className={iconButtonClass} disabled={index === 0}
					aria-label={t("composer.favorites.moveUp", { model: label })} onClick={() => move(-1)}
				><ArrowUp className="size-3.5" /></button>
				<button type="button" className={iconButtonClass} disabled={index === favorites.length - 1}
					aria-label={t("composer.favorites.moveDown", { model: label })} onClick={() => move(1)}
				><ArrowDown className="size-3.5" /></button>
			</div>
			<button type="button" className={`${iconButtonClass} hover:text-destructive`}
				aria-label={t("composer.favorites.remove", { model: label })}
				onClick={() => removeModelFavorite(favorite)}
			><Trash2 className="size-3.5" /></button>
			</div>
		</Reorder.Item>
	);
}

export function ModelFavoritesDialog({ providers, initialFavorite, onClose, onRestoreFocus }: {
	providers: ProviderCatalog["providers"];
	initialFavorite: ModelFavorite | null;
	onClose: () => void;
	onRestoreFocus: () => void;
}) {
	const { t } = useTranslation("common");
	const id = useId();
	const favorites = useModelFavorites();
	const [providerId, setProviderId] = useState(initialFavorite?.providerId ?? "");
	const [modelId, setModelId] = useState(initialFavorite?.modelId ?? "");
	const [effort, setEffort] = useState(initialFavorite?.effort ?? "medium");
	const enabledProviders = providers.filter(isProviderEnabled);
	const provider = enabledProviders.find((entry) => entry.id === providerId) ?? enabledProviders[0];
	const model = provider?.models.find((entry) => entry.id === modelId) ?? provider?.models[0];
	const selectedEffort = model?.effortLevels.length
		? effort === "ultrathink" ? effort : clampEffort(effort, model.effortLevels)
		: null;
	const candidate = provider && model
		? { providerId: provider.id, modelId: model.id, effort: selectedEffort } : null;
	const alreadySaved = candidate && favorites.some((entry) => modelFavoriteKey(entry) === modelFavoriteKey(candidate));

	return (
		<Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
			<DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-xl"
				onCloseAutoFocus={(event) => { event.preventDefault(); onRestoreFocus(); }}>
				<DialogHeader>
					<DialogTitle>{t("composer.favorites.edit")}</DialogTitle>
					<DialogDescription>{t("composer.favorites.description")}</DialogDescription>
				</DialogHeader>
				{favorites.length ? (
					<Reorder.Group axis="y" values={favorites.map(modelFavoriteKey)}
						onReorder={(keys: string[]) => {
							const byKey = new Map(favorites.map((entry) => [modelFavoriteKey(entry), entry]));
							saveModelFavorites(keys.map((key) => byKey.get(key)!));
						}}
						layoutScroll className="max-h-[40dvh] space-y-2 overflow-y-auto p-1"
						aria-label={t("composer.favorites.title")}
					>
						{favorites.map((favorite, index) => <FavoriteRow key={modelFavoriteKey(favorite)}
							favorite={favorite} providers={providers} favorites={favorites} index={index} />)}
					</Reorder.Group>
				) : <p className="rounded-lg border border-dashed border-border p-4 text-sm text-muted-foreground">
					{t("composer.favorites.empty")}
				</p>}
				<form className="space-y-3 rounded-lg border border-border p-3"
					onSubmit={(event) => {
						event.preventDefault();
						if (candidate && !alreadySaved) addModelFavorite(candidate);
					}}
				>
					<div className="grid grid-cols-2 gap-3">
						<label htmlFor={`${id}-provider`} className="space-y-1 text-xs text-muted-foreground">
							<span>{t("composer.favorites.provider")}</span>
							<select id={`${id}-provider`} className={selectClass} value={provider?.id ?? ""}
								onChange={(event) => { setProviderId(event.target.value); setModelId(""); }}>
								{enabledProviders.map((entry) => <option key={entry.id} value={entry.id}>{entry.label}</option>)}
							</select>
						</label>
						<label htmlFor={`${id}-effort`} className="space-y-1 text-xs text-muted-foreground">
							<span>{t("composer.execution.effort")}</span>
							<select id={`${id}-effort`} className={selectClass} value={selectedEffort ?? ""}
								disabled={!model?.effortLevels.length} onChange={(event) => setEffort(event.target.value)}>
								{model?.effortLevels.length ? [...model.effortLevels, "ultrathink"].map((value) =>
									<option key={value} value={value}>{t(`composer.effort.${value}`, { defaultValue: getEffortDisplay(value).label })}</option>,
								) : <option value="">{t("composer.favorites.managed")}</option>}
							</select>
						</label>
					</div>
					<label htmlFor={`${id}-model`} className="block space-y-1 text-xs text-muted-foreground">
						<span>{t("composer.execution.model")}</span>
						<select id={`${id}-model`} className={selectClass} value={model?.id ?? ""}
							onChange={(event) => setModelId(event.target.value)}>
							{provider?.models.map((entry) => <option key={entry.id} value={entry.id}>{entry.label}</option>)}
						</select>
					</label>
					<Button type="submit" variant="outline" size="sm" disabled={!candidate || Boolean(alreadySaved)}>
						<Plus className="size-3.5" />
						{t(alreadySaved ? "composer.favorites.saved" : "composer.favorites.add")}
					</Button>
				</form>
				<DialogFooter className="items-center sm:justify-between">
					<p className="text-xs text-muted-foreground">{t("composer.favorites.autoSaved")}</p>
					<DialogClose asChild><Button size="sm">{t("composer.favorites.done")}</Button></DialogClose>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
