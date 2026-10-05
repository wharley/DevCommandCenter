import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Folder, GitBranch, RotateCcw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { Repository } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";
import {
	PROJECT_COLOR_OPTIONS,
	PROJECT_ICON_OPTIONS,
	PROJECT_MASCOT_OPTIONS,
	ProjectIdentityGlyph,
	isKnownProjectColor,
	isKnownProjectIcon,
} from "./project-identity";
import { repositoryDisplayName } from "./repository-display-name";

const AUTO = "auto";

const pickTileClass = (selected: boolean) =>
	cn(
		"grid h-9 place-items-center rounded-lg border transition-colors hover:bg-muted/40",
		selected ? "border-foreground/30 bg-muted/45 ring-1 ring-foreground/10" : "border-border/55",
	);

const glyphValue = (value: string) => (value === AUTO ? null : value);

function normalizedDisplayName(value: string, repository: Repository) {
	const trimmed = value.trim();
	if (!trimmed || trimmed.toLocaleLowerCase() === repository.name.trim().toLocaleLowerCase()) {
		return null;
	}
	return trimmed;
}

export function ProjectEditDialog({
	repository,
	open,
	onOpenChange,
	onSave,
}: {
	repository: Repository | null;
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onSave: (input: {
		repositoryId: string;
		displayName: string | null;
		icon: string | null;
		color: string | null;
	}) => Promise<void>;
}) {
	const { t } = useTranslation("common");
	const [name, setName] = useState("");
	// "auto" = nothing stored: the mascot and color come from the project path.
	const [icon, setIcon] = useState(AUTO);
	const [color, setColor] = useState(AUTO);
	const [isSaving, setIsSaving] = useState(false);

	useEffect(() => {
		if (open && repository) {
			setName(repositoryDisplayName(repository));
			setIcon(isKnownProjectIcon(repository.icon) ? repository.icon : AUTO);
			setColor(isKnownProjectColor(repository.color) ? repository.color : AUTO);
			setIsSaving(false);
		}
	}, [open, repository]);

	const nextDisplayName = repository
		? normalizedDisplayName(name, repository)
		: null;
	const currentDisplayName = repository?.displayName?.trim() || null;
	const nextIcon = icon === AUTO ? null : icon;
	const nextColor = color === AUTO ? null : color;
	const currentIcon = repository?.icon?.trim() || null;
	const currentColor = repository?.color?.trim() || null;
	const isDirty = useMemo(
		() =>
			nextDisplayName !== currentDisplayName ||
			nextIcon !== currentIcon ||
			nextColor !== currentColor,
		[
			currentColor,
			currentDisplayName,
			currentIcon,
			nextColor,
			nextDisplayName,
			nextIcon,
		],
	);

	async function handleSubmit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		if (!repository || !isDirty || isSaving) return;
		setIsSaving(true);
		try {
			await onSave({
				repositoryId: repository.id,
				displayName: nextDisplayName,
				icon: nextIcon,
				color: nextColor,
			});
			toast.success(t("projectEditor.saved"));
			onOpenChange(false);
		} catch (error) {
			toast.error(t("projectEditor.saveError"), {
				description: error instanceof Error ? error.message : undefined,
			});
		} finally {
			setIsSaving(false);
		}
	}

	return (
		<Dialog open={open} onOpenChange={(nextOpen) => !isSaving && onOpenChange(nextOpen)}>
			<DialogContent
				showCloseButton={!isSaving}
				className="w-[min(calc(100vw-2rem),32rem)] max-w-[32rem] gap-5 p-5"
			>
				<DialogHeader className="space-y-1">
					<DialogTitle className="text-[15px] font-medium tracking-[-0.015em]">
						{t("projectEditor.title")}
					</DialogTitle>
					<DialogDescription className="text-[12px] leading-5">
						{t("projectEditor.description")}
					</DialogDescription>
				</DialogHeader>

				{repository ? (
					<form className="space-y-5" onSubmit={handleSubmit}>
						<div className="grid grid-cols-[auto_1fr] items-center gap-3">
							<ProjectIdentityGlyph
								icon={glyphValue(icon)}
								color={glyphValue(color)}
								seed={repository.rootPath}
								active
								size="lg"
								className="mb-0.5"
							/>
							<div className="min-w-0 space-y-2">
								<div className="flex items-center justify-between gap-3">
									<Label htmlFor="project-display-name" className="text-[12px] font-medium">
										{t("projectEditor.nameLabel")}
									</Label>
									<Button
										type="button"
										variant="ghost"
										size="sm"
										className="h-7 gap-1.5 px-2 text-[11px] text-muted-foreground"
										disabled={name === repository.name || isSaving}
										onClick={() => setName(repository.name)}
									>
										<RotateCcw className="size-3" aria-hidden />
										{t("projectEditor.useRepositoryName")}
									</Button>
								</div>
								<Input
									id="project-display-name"
									value={name}
									autoFocus
									maxLength={80}
									disabled={isSaving}
									placeholder={repository.name}
									onChange={(event) => setName(event.target.value)}
								/>
								<p className="text-[10.5px] leading-4 text-muted-foreground">
									{t("projectEditor.nameHint")}
								</p>
							</div>
						</div>

						<fieldset className="min-w-0 space-y-2">
							<legend className="text-[12px] font-medium">
								{t("projectEditor.iconLabel")}
							</legend>
							<p className="text-[10.5px] leading-4 text-muted-foreground">
								{t("projectEditor.faunaLabel")}
							</p>
							<div className="grid grid-cols-6 gap-1.5">
								<button
									type="button"
									aria-label={t("projectEditor.auto")}
									title={t("projectEditor.autoHint")}
									aria-pressed={icon === AUTO}
									disabled={isSaving}
									className={cn(pickTileClass(icon === AUTO), "text-[10px] font-medium")}
									onClick={() => setIcon(AUTO)}
								>
									{t("projectEditor.autoShort")}
								</button>
								{PROJECT_MASCOT_OPTIONS.map((option) => (
									<button
										type="button"
										key={option}
										aria-label={t(`projectEditor.icons.${option}`)}
										title={t(`projectEditor.icons.${option}`)}
										aria-pressed={icon === option}
										disabled={isSaving}
										className={pickTileClass(icon === option)}
										onClick={() => setIcon(option)}
									>
										<ProjectIdentityGlyph
											icon={option}
											color={glyphValue(color)}
											seed={repository.rootPath}
											size="sm"
										/>
									</button>
								))}
							</div>
							<p className="pt-1 text-[10.5px] leading-4 text-muted-foreground">
								{t("projectEditor.symbolsLabel")}
							</p>
							<div className="grid grid-cols-6 gap-1.5">
								{PROJECT_ICON_OPTIONS.map((option) => (
									<button
										type="button"
										key={option.id}
										aria-label={t(`projectEditor.icons.${option.id}`)}
										title={t(`projectEditor.icons.${option.id}`)}
										aria-pressed={icon === option.id}
										disabled={isSaving}
										className={pickTileClass(icon === option.id)}
										onClick={() => setIcon(option.id)}
									>
										<ProjectIdentityGlyph
											icon={option.id}
											color={glyphValue(color)}
											seed={repository.rootPath}
											size="sm"
										/>
									</button>
								))}
							</div>
						</fieldset>

						<fieldset className="min-w-0 space-y-2">
							<legend className="text-[12px] font-medium">
								{t("projectEditor.colorLabel")}
							</legend>
							<div className="grid grid-cols-7 gap-1.5">
								<button
									type="button"
									aria-label={t("projectEditor.auto")}
									title={t("projectEditor.autoHint")}
									aria-pressed={color === AUTO}
									disabled={isSaving}
									className={cn(pickTileClass(color === AUTO), "text-[10px] font-medium")}
									onClick={() => setColor(AUTO)}
								>
									{t("projectEditor.autoShort")}
								</button>
								{PROJECT_COLOR_OPTIONS.map((option) => (
									<button
										type="button"
										key={option}
										aria-label={t(`projectEditor.colors.${option}`)}
										title={t(`projectEditor.colors.${option}`)}
										aria-pressed={color === option}
										disabled={isSaving}
										className={pickTileClass(color === option)}
										onClick={() => setColor(option)}
									>
										<ProjectIdentityGlyph
											icon={glyphValue(icon)}
											color={option}
											seed={repository.rootPath}
											size="sm"
										/>
									</button>
								))}
							</div>
						</fieldset>

						<div className="space-y-2 rounded-xl border border-border/60 bg-muted/15 p-3">
							<p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
								{t("projectEditor.technicalIdentity")}
							</p>
							<div className="grid min-w-0 grid-cols-[auto_1fr] items-center gap-x-2.5 gap-y-2 text-[11px]">
								<Folder className="size-3.5 text-muted-foreground" aria-hidden />
								<div className="min-w-0">
									<div className="truncate font-medium text-foreground">{repository.name}</div>
									<div className="truncate font-mono text-[10px] text-muted-foreground">
										{repository.rootPath}
									</div>
								</div>
								<GitBranch className="size-3.5 text-muted-foreground" aria-hidden />
								<span className="truncate">{repository.baseBranch}</span>
							</div>
						</div>

						<DialogFooter className="gap-2 sm:gap-2">
							<Button
								type="button"
								variant="ghost"
								disabled={isSaving}
								onClick={() => onOpenChange(false)}
							>
								{t("projectEditor.cancel")}
							</Button>
							<Button type="submit" disabled={!isDirty || isSaving}>
								{isSaving ? t("projectEditor.saving") : t("projectEditor.save")}
							</Button>
						</DialogFooter>
					</form>
				) : null}
			</DialogContent>
		</Dialog>
	);
}
