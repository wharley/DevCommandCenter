import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { PROJECT_LOGO_ACCEPT, projectLogoFromFile } from "./project-logo";
import { Folder, FolderPlus, GitBranch, RotateCcw, X } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
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

function folderName(path: string) {
	return path.split(/[\\/]/u).filter(Boolean).at(-1) ?? path;
}

export function ProjectEditDialog({
	repository,
	open,
	onOpenChange,
	onSave,
	linkedFolders = [],
	contextFolders = [],
	onSaveLinkedFolders,
}: {
	repository: Repository | null;
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onSave: (input: {
		repositoryId: string;
		displayName: string | null;
		icon: string | null;
		color: string | null;
		logo: string | null;
	}) => Promise<void>;
	/** Folders this project brings into every new task. */
	linkedFolders?: string[];
	/** The saved linked folders without Git: they join tasks as context. */
	contextFolders?: string[];
	onSaveLinkedFolders?: (repositoryId: string, paths: string[]) => Promise<void>;
}) {
	const { t } = useTranslation("common");
	const [name, setName] = useState("");
	// "auto" = nothing stored: the mascot and color come from the project path.
	const [icon, setIcon] = useState(AUTO);
	const [color, setColor] = useState(AUTO);
	const [logo, setLogo] = useState<string | null>(null);
	const [folders, setFolders] = useState<string[]>([]);
	const linkedFoldersKey = linkedFolders.join("\n");
	const [isSaving, setIsSaving] = useState(false);
	const logoInputRef = useRef<HTMLInputElement>(null);

	useEffect(() => {
		if (open && repository) {
			setName(repositoryDisplayName(repository));
			setIcon(isKnownProjectIcon(repository.icon) ? repository.icon : AUTO);
			setColor(isKnownProjectColor(repository.color) ? repository.color : AUTO);
			setLogo(repository.logo ?? null);
			setFolders(linkedFoldersKey ? linkedFoldersKey.split("\n") : []);
			setIsSaving(false);
		}
		// The saved folders, by value: a fresh array for the same folders must
		// not reset what the person is editing.
	}, [open, repository, linkedFoldersKey]);

	const nextDisplayName = repository
		? normalizedDisplayName(name, repository)
		: null;
	const currentDisplayName = repository?.displayName?.trim() || null;
	const nextIcon = icon === AUTO ? null : icon;
	const nextColor = color === AUTO ? null : color;
	const currentIcon = repository?.icon?.trim() || null;
	const currentColor = repository?.color?.trim() || null;
	const currentLogo = repository?.logo ?? null;
	const foldersDirty =
		folders.length !== linkedFolders.length ||
		folders.some((folder, index) => folder !== linkedFolders[index]);
	const identityDirty = useMemo(
		() =>
			nextDisplayName !== currentDisplayName ||
			nextIcon !== currentIcon ||
			nextColor !== currentColor ||
			logo !== currentLogo,
		[
			currentLogo,
			logo,
			currentColor,
			currentDisplayName,
			currentIcon,
			nextColor,
			nextDisplayName,
			nextIcon,
		],
	);
	const isDirty = identityDirty || foldersDirty;

	async function handleAddFolder() {
		const selected = await openDialog({ directory: true, multiple: false });
		const path = typeof selected === "string" ? selected.trim() : "";
		if (!path) return;
		setFolders((current) => (current.includes(path) ? current : [...current, path]));
	}

	async function handleSubmit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		if (!repository || !isDirty || isSaving) return;
		setIsSaving(true);
		try {
			if (identityDirty) {
				await onSave({
					repositoryId: repository.id,
					displayName: nextDisplayName,
					icon: nextIcon,
					color: nextColor,
					logo,
				});
			}
			if (foldersDirty && onSaveLinkedFolders) {
				await onSaveLinkedFolders(repository.id, folders);
			}
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
				className="flex max-h-[min(88vh,46rem)] w-[min(calc(100vw-2rem),52rem)] max-w-[52rem] flex-col gap-5 overflow-hidden p-5 sm:max-w-[52rem]"
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
					<form className="flex min-h-0 flex-1 flex-col gap-5" onSubmit={handleSubmit}>
						{/* Identity on the left, folders and technical facts on the right;
						    one column on narrow windows. Only this area scrolls, so the
						    actions stay in reach. */}
						<div className="-mr-2 grid min-h-0 flex-1 gap-6 overflow-y-auto pr-2 md:grid-cols-2">
							<div className="min-w-0 space-y-5">
								<div className="grid grid-cols-[auto_1fr] items-center gap-3">
									<ProjectIdentityGlyph
										icon={icon === "logo" ? logo : glyphValue(icon)}
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
										{t("projectEditor.logoLabel")}
									</p>
									<div className="flex items-center gap-1.5">
										{logo ? (
											<button
												type="button"
												aria-label={t("projectEditor.logoLabel")}
												aria-pressed={icon === "logo"}
												disabled={isSaving}
												className={cn(pickTileClass(icon === "logo"), "w-9")}
												onClick={() => setIcon("logo")}
											>
												<ProjectIdentityGlyph icon={logo} color={glyphValue(color)} size="sm" />
											</button>
										) : null}
										<Button
											type="button"
											variant="outline"
											size="sm"
											className="h-9 text-[11px]"
											disabled={isSaving}
											onClick={() => logoInputRef.current?.click()}
										>
											{logo ? t("projectEditor.replaceLogo") : t("projectEditor.uploadLogo")}
										</Button>
										{logo ? (
											<Button
												type="button"
												variant="ghost"
												size="sm"
												className="h-9 text-[11px] text-muted-foreground"
												disabled={isSaving}
												onClick={() => {
													setLogo(null);
													if (icon === "logo") setIcon(AUTO);
												}}
											>
												{t("projectEditor.removeLogo")}
											</Button>
										) : null}
										<span className="min-w-0 text-[10px] leading-4 text-muted-foreground">
											{t("projectEditor.logoHint")}
										</span>
										<input
											ref={logoInputRef}
											type="file"
											accept={PROJECT_LOGO_ACCEPT}
											className="hidden"
											onChange={(event) => {
												const file = event.target.files?.[0];
												event.target.value = "";
												if (!file) return;
												void projectLogoFromFile(file)
													.then((dataUrl) => {
														setLogo(dataUrl);
														setIcon("logo");
													})
													.catch(() => toast.error(t("projectEditor.logoError")));
											}}
										/>
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

							</div>

							<div className="min-w-0 space-y-5">
								{onSaveLinkedFolders ? (
									<fieldset className="min-w-0 space-y-2">
										<legend className="text-[12px] font-medium">
											{t("projectEditor.linkedFoldersLabel")}
										</legend>
										<p className="text-[11px] leading-4 text-muted-foreground">
											{t("projectEditor.linkedFoldersHint")}
										</p>
										{folders.length > 0 ? (
											<ul className="space-y-1">
												{folders.map((folder) => (
													<li
														key={folder}
														className="flex min-w-0 items-center gap-2 rounded-lg border border-border/55 px-2.5 py-1.5 text-[11px]"
													>
														<Folder className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
														<span className="min-w-0 flex-1">
															<span className="block truncate font-medium text-foreground">
																{folderName(folder)}
																{contextFolders.includes(folder) ? (
																	<span className="ml-1.5 rounded-full bg-muted px-1.5 py-px text-[9.5px] font-medium text-muted-foreground">
																		{t("projectEditor.contextOnly")}
																	</span>
																) : null}
															</span>
															<span className="block truncate font-mono text-[10px] text-muted-foreground">
																{folder}
															</span>
														</span>
														<button
															type="button"
															disabled={isSaving}
															aria-label={t("projectEditor.removeLinkedFolder", {
																folder: folderName(folder),
															})}
															className="grid size-6 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
															onClick={() =>
																setFolders((current) =>
																	current.filter((candidate) => candidate !== folder),
																)
															}
														>
															<X className="size-3.5" aria-hidden />
														</button>
													</li>
												))}
											</ul>
										) : null}
										<Button
											type="button"
											variant="outline"
											size="sm"
											className="gap-1.5"
											disabled={isSaving}
											onClick={() => void handleAddFolder()}
										>
											<FolderPlus className="size-3.5" aria-hidden />
											{t("projectEditor.addLinkedFolder")}
										</Button>
									</fieldset>
								) : null}

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
							</div>
						</div>

						<DialogFooter className="shrink-0 gap-2 sm:gap-2">
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
