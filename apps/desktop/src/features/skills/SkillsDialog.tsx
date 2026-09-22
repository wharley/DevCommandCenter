import {
	ArrowLeft,
	FileText,
	FolderSearch,
	Pencil,
	Plus,
	Trash2,
	Workflow,
	Sparkles,
	Search,
	Loader2,
	RefreshCw,
	ChevronRight,
	Download,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
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
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { SkillTargetPicker, SKILL_TARGETS } from "./skill-target-picker";
import "./skills.css";
import { DCC_SKILL_CATALOG } from "./skill-catalog";
import {
	compileSkills,
	deleteSkill,
	detectSkillContext,
	listSkills,
	importSkill,
	previewSkillImport,
	saveSkill,
	type SkillContextDetection,
	type SkillImportPreview,
	type SkillRecord,
	type SkillTargetAgent,
} from "@/lib/skills-api";
import { skillsErrorMessage } from "./skills-error";

export type SkillsDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onSkillsChanged?: () => void;
	/** Active checkout (`worktreePath` for protected worktrees, `rootPath` for local-direct). */
	projectRoot: string | null;
	/** Exact workspace row used to authorize mutations when roots are shared. */
	workspaceId: string | null;
};

const NAME_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

type FormState = {
	editingExisting: boolean;
	fromCatalog?: boolean;
	githubImport?: SkillImportPreview;
	sourceUrl?: string | null;
	name: string;
	description: string;
	body: string;
	targetAgents: SkillTargetAgent[];
	disableModelInvocation: boolean;
};

function emptyForm(): FormState {
	return {
		editingExisting: false,
		name: "",
		description: "",
		body: "",
		targetAgents: ["claude"],
		disableModelInvocation: false,
	};
}

function catalogForm(skill: SkillRecord, existing?: SkillRecord): FormState {
	return {
		editingExisting: Boolean(existing),
		fromCatalog: true,
		name: skill.name,
		description: skill.description,
		body: skill.body,
		targetAgents: [...(existing?.targetAgents ?? skill.targetAgents)],
		disableModelInvocation:
			existing?.disableModelInvocation ?? skill.disableModelInvocation,
	};
}

export function SkillsDialog({
	open,
	onOpenChange,
	onSkillsChanged,
	projectRoot,
	workspaceId,
}: SkillsDialogProps) {
	const { t } = useTranslation("common");
	const [search, setSearch] = useState("");
	const [section, setSection] = useState<"library" | "detected" | "catalog">(
		"library",
	);
	const [loadError, setLoadError] = useState<string | null>(null);
	const searchRef = useRef<HTMLInputElement>(null);
	const catalogRef = useRef<HTMLButtonElement>(null);
	const [skills, setSkills] = useState<SkillRecord[]>([]);
	const [detections, setDetections] = useState<SkillContextDetection[]>([]);
	const [loading, setLoading] = useState(false);
	const [form, setForm] = useState<FormState | null>(null);
	const [busy, setBusy] = useState(false);
	const [githubUrl, setGithubUrl] = useState("");

	const refresh = useCallback(async () => {
		if (!projectRoot) {
			setSkills([]);
			setDetections([]);
			return;
		}
		setLoading(true);
		setLoadError(null);
		try {
			const [nextSkills, nextDetections] = await Promise.all([
				listSkills(projectRoot),
				detectSkillContext(projectRoot, projectRoot),
			]);
			setSkills(nextSkills);
			setDetections(nextDetections);
		} catch (error) {
			setLoadError(skillsErrorMessage(error, t("skills.toast.loadError")));
		} finally {
			setLoading(false);
		}
	}, [projectRoot, t]);

	useEffect(() => {
		if (open) {
			setForm(null);
			setSearch("");
			setGithubUrl("");
			setSection("library");
			void refresh();
		}
	}, [open, refresh]);

	const recompile = useCallback(async () => {
		if (!projectRoot || !workspaceId) {
			return;
		}
		await compileSkills(projectRoot, workspaceId);
	}, [projectRoot, workspaceId]);

	const handleSave = useCallback(async () => {
		if (!projectRoot || !workspaceId || !form) {
			return;
		}
		const name = form.name.trim();
		if (!NAME_PATTERN.test(name)) {
			toast.error(t("skills.toast.nameInvalid"));
			return;
		}
		if (!form.description.trim()) {
			toast.error(t("skills.toast.descriptionRequired"));
			return;
		}
		if (form.targetAgents.length === 0) {
			toast.error(t("skills.toast.pickTarget"));
			return;
		}
		setBusy(true);
		try {
			const skill: SkillRecord = {
				name,
				description: form.description.trim(),
				body: form.body,
				targetAgents: form.targetAgents,
				disableModelInvocation: form.disableModelInvocation,
				scope: "project",
				sourceUrl: form.sourceUrl ?? null,
			};
			if (form.githubImport) {
				await importSkill(projectRoot, workspaceId, form.githubImport, skill);
			} else {
				await saveSkill(projectRoot, workspaceId, skill);
			}
			await recompile();
			toast.success(t("skills.toast.saved", { name }));
			setForm(null);
			setSection("library");
			setSearch("");
			await refresh();
			onSkillsChanged?.();
		} catch (error) {
			toast.error(skillsErrorMessage(error, t("skills.toast.saveError")));
		} finally {
			setBusy(false);
		}
	}, [projectRoot, workspaceId, form, onSkillsChanged, recompile, refresh, t]);

	const handleDelete = useCallback(
		async (name: string) => {
			if (!projectRoot || !workspaceId) {
				return;
			}
			setBusy(true);
			try {
				await deleteSkill(projectRoot, workspaceId, name);
				await recompile();
				toast.success(t("skills.toast.deleted", { name }));
				await refresh();
				onSkillsChanged?.();
			} catch (error) {
				toast.error(skillsErrorMessage(error, t("skills.toast.deleteError")));
			} finally {
				setBusy(false);
			}
		},
		[onSkillsChanged, projectRoot, workspaceId, recompile, refresh, t],
	);

	const detectionDescription = useCallback(
		(item: SkillContextDetection) => {
			switch (item.kind) {
				case "dcc_source":
					return t("skills.detected.behavior.dccSource");
				case "instructions_file":
					return t("skills.detected.behavior.instructionsFile");
				case "claude_skills":
					return t("skills.detected.behavior.claudeSkills");
				case "cursor_rules":
					return t("skills.detected.behavior.cursorRules");
				case "codex_skills":
					return t("skills.detected.behavior.codexSkills");
				case "grok_skills":
					return t("skills.detected.behavior.grokSkills");
				default:
					return t("skills.detected.behavior.generic");
			}
		},
		[t],
	);

	const filteredSkills = skills.filter((skill) =>
		`${skill.name} ${skill.description} ${skill.targetAgents.map((target) => SKILL_TARGETS[target]?.name ?? target).join(" ")}`
			.toLocaleLowerCase()
			.includes(search.trim().toLocaleLowerCase()),
	);
	const backToLibrary = () => {
		setForm(null);
		requestAnimationFrame(() => {
			if (section === "catalog") catalogRef.current?.focus();
			else searchRef.current?.focus();
		});
	};

	const handleGithubImport = async () => {
		if (!workspaceId || !githubUrl.trim()) return;
		setBusy(true);
		try {
			const preview = await previewSkillImport(githubUrl.trim());
			setForm({
				editingExisting: false,
				githubImport: preview,
				name: preview.skill.name,
				description: preview.skill.description,
				body: preview.skill.body,
				targetAgents: ["claude"],
				disableModelInvocation: false,
				sourceUrl: preview.sourceUrl,
			});
			setGithubUrl("");
		} catch (error) {
			toast.error(skillsErrorMessage(error, t("skills.github.importError")));
		} finally {
			setBusy(false);
		}
	};

	return (
		<Dialog
			open={open}
			onOpenChange={(next) => {
				if (!busy) onOpenChange(next);
			}}
		>
			<DialogContent className="skills-dialog" showCloseButton={!busy}>
				<DialogHeader className="skills-header">
					<div className="skills-heading-row">
						<span className="skills-heading-mark">
							<Sparkles size={20} aria-hidden />
						</span>
						<div className="min-w-0">
							<DialogTitle>{t("skills.title")}</DialogTitle>
							<DialogDescription>
								{t("skills.design.subtitle")}
							</DialogDescription>
						</div>
					</div>
					{projectRoot && (
						<div className="skills-project-context">
							<span>{t("skills.design.project")}</span>
							<code title={projectRoot}>
								{projectRoot.split(/[\\/]/).filter(Boolean).at(-1)}
							</code>
						</div>
					)}
				</DialogHeader>
				{!projectRoot ? (
					<div className="skills-empty">
						<FolderSearch size={30} aria-hidden />
						<p>{t("skills.selectProject")}</p>
					</div>
				) : form ? (
					<>
						<div className="skills-form-heading">
							<Button
								variant="ghost"
								size="icon-sm"
								aria-label={t(
									section === "catalog"
										? "skills.catalog.back"
										: "skills.design.back",
								)}
								onClick={backToLibrary}
								disabled={busy}
							>
								<ArrowLeft size={15} />
							</Button>
							<div>
								<h2>
									{form.editingExisting
										? t("skills.design.edit")
										: form.githubImport
											? t("skills.github.reviewTitle")
											: t(
												form.fromCatalog
													? "skills.catalog.review"
													: "skills.newSkill",
											)}
								</h2>
								<p>
									{t(
										form.githubImport
											? "skills.github.reviewHint"
											: form.fromCatalog
											? form.editingExisting
												? "skills.catalog.updateHint"
												: "skills.catalog.reviewHint"
											: "skills.design.formHint",
									)}
								</p>
							</div>
						</div>
						<div className="skills-scroll">
							<fieldset disabled={busy} className="skills-form">
								<section className="skills-form-section">
									<div className="skills-fields-grid">
										<div>
											<Label htmlFor="skill-name">
												{t("skills.fields.name")}
											</Label>
											<Input
												id="skill-name"
												maxLength={64}
												autoFocus={!form.editingExisting && !form.fromCatalog}
												placeholder={t("skills.fields.namePlaceholder")}
												value={form.name}
												disabled={form.editingExisting || form.fromCatalog}
												onChange={(e) =>
													setForm({ ...form, name: e.target.value })
												}
											/>
										</div>
										<div>
											<Label htmlFor="skill-desc">
												{t("skills.fields.description")}
											</Label>
											<Input
												id="skill-desc"
												autoFocus={form.editingExisting || form.fromCatalog}
												placeholder={t("skills.fields.descriptionPlaceholder")}
												value={form.description}
												onChange={(e) =>
													setForm({ ...form, description: e.target.value })
												}
											/>
										</div>
									</div>
									<div className="skills-body-field">
										<Label htmlFor="skill-body">
											{t("skills.fields.body")}
										</Label>
										<Textarea
											id="skill-body"
											placeholder={t("skills.fields.bodyPlaceholder")}
											value={form.body}
											onChange={(e) =>
												setForm({ ...form, body: e.target.value })
											}
										/>
									</div>
								</section>
								{form.githubImport && (
									<section className="skills-import-review" aria-live="polite">
										<div>
											<strong>{t("skills.github.recognized")}</strong>
											<span>{t("skills.github.resourceCount", { count: form.githubImport.files.length })}</span>
										</div>
										<a href={form.githubImport.sourceUrl} target="_blank" rel="noreferrer">
											{form.githubImport.sourceUrl}
										</a>
										{form.githubImport.warnings.map((warning) => (
											<p key={warning}>{warning}</p>
										))}
										{skills.some((skill) => skill.name === form.name.trim()) && (
											<p>{t("skills.github.duplicateName")}</p>
										)}
										{form.githubImport.files.length > 0 && (
											<ul>
												{form.githubImport.files.slice(0, 8).map((file) => (
													<li key={file.path}><code>{file.path}</code></li>
												))}
												{form.githubImport.files.length > 8 && (
													<li>{t("skills.github.moreFiles", { count: form.githubImport.files.length - 8 })}</li>
												)}
											</ul>
										)}
										<p>{t("skills.github.trustHint")}</p>
									</section>
								)}
								<section className="skills-form-section">
									<h3 className="skills-section-title">
										{t("skills.fields.targetAgents")}
									</h3>
									<SkillTargetPicker
										value={form.targetAgents}
										onChange={(targetAgents) =>
											setForm({ ...form, targetAgents })
										}
									/>
									<p className="skills-field-hint">{t("skills.targetsHint")}</p>
								</section>
								<div className="skills-invocation">
									<div>
										<Label htmlFor="skill-invocation">
											{t("skills.disableInvocation.title")}
										</Label>
										<p>{t("skills.disableInvocation.body")}</p>
									</div>
									<Switch
										id="skill-invocation"
										checked={form.disableModelInvocation}
										onCheckedChange={(checked) =>
											setForm({ ...form, disableModelInvocation: checked })
										}
									/>
								</div>
							</fieldset>
						</div>
						<DialogFooter className="skills-footer">
							<span className="skills-save-hint">
								{t("skills.compileNote")}
							</span>
							<Button variant="ghost" onClick={backToLibrary} disabled={busy}>
								{t("skills.cancel")}
							</Button>
								<Button onClick={() => void handleSave()} disabled={busy}>
								{busy && <Loader2 className="size-3.5 animate-spin" />}
								{t(
									form.fromCatalog && !form.editingExisting
										? "skills.catalog.add"
										: form.githubImport
											? "skills.github.confirmImport"
											: "skills.save",
								)}
							</Button>
						</DialogFooter>
					</>
				) : (
					<>
						<div
							className="skills-navigation"
							role="group"
							aria-label={t("skills.design.navigation")}
						>
							<button
								type="button"
								aria-pressed={section === "library"}
								onClick={() => setSection("library")}
							>
								<Sparkles size={14} aria-hidden />
								{t("skills.design.library")}
								<span>{loading ? "—" : skills.length}</span>
							</button>
							<button
								type="button"
								aria-pressed={section === "detected"}
								onClick={() => setSection("detected")}
							>
								<FolderSearch size={14} aria-hidden />
								{t("skills.detected.title")}
								<span>{loading ? "—" : detections.length}</span>
							</button>
							<button
								ref={catalogRef}
								type="button"
								aria-pressed={section === "catalog"}
								onClick={() => setSection("catalog")}
							>
								<Workflow size={14} aria-hidden />
								{t("skills.catalog.title")}
								<span>{DCC_SKILL_CATALOG.length}</span>
							</button>
							<Button
								variant="ghost"
								size="icon-sm"
								aria-label={t("skills.design.refresh")}
								disabled={loading || busy}
								onClick={() => void refresh()}
							>
								<RefreshCw
									size={14}
									className={loading ? "animate-spin" : ""}
								/>
							</Button>
						</div>
						{section === "library" && (
							<>
							<div className="skills-toolbar">
								<label className="skills-search">
									<Search size={15} aria-hidden />
									<input
										ref={searchRef}
										value={search}
										onChange={(event) => setSearch(event.target.value)}
										placeholder={t("skills.design.search")}
										aria-label={t("skills.design.search")}
									/>
								</label>
								<Button
									size="sm"
									disabled={busy || loading || !workspaceId}
									onClick={() => setForm(emptyForm())}
								>
									<Plus size={15} />
									{t("skills.newSkill")}
								</Button>
							</div>
							<div className="skills-github-import">
								<label htmlFor="skills-github-url">{t("skills.github.urlLabel")}</label>
								<div>
									<Input
										id="skills-github-url"
										value={githubUrl}
										onChange={(event) => setGithubUrl(event.target.value)}
										placeholder={t("skills.github.urlPlaceholder")}
										disabled={busy || loading || !workspaceId}
									/>
									<Button
										variant="outline"
										size="sm"
										disabled={busy || loading || !workspaceId || !githubUrl.trim()}
										onClick={() => void handleGithubImport()}
									>
										{busy ? <Loader2 className="size-3.5 animate-spin" /> : <Download size={14} />}
										{t("skills.github.previewImport")}
									</Button>
								</div>
								<p>{t("skills.github.urlHint")}</p>
							</div>
							</>
						)}
						<div className="skills-scroll" aria-busy={loading}>
							{loadError ? (
								<div role="alert" className="skills-load-error">
									<p>{loadError}</p>
									<Button
										variant="outline"
										size="sm"
										onClick={() => void refresh()}
										disabled={loading}
									>
										{t("skills.design.retry")}
									</Button>
								</div>
							) : loading ? (
								<div className="skills-empty" role="status">
									<Loader2 className="animate-spin" size={24} />
									<p>{t("skills.loading")}</p>
								</div>
							) : section === "catalog" ? (
								<div className="skills-catalog">
									<p className="skills-detection-hint">
										{t("skills.catalog.hint")}
									</p>
									{DCC_SKILL_CATALOG.map((entry) => {
										const installed = skills.find(
											(skill) => skill.name === entry.skill.name,
										);
										return (
											<article className="skills-preset" key={entry.skill.name}>
												<span className="skills-preset-mark">
													<Workflow size={19} aria-hidden />
												</span>
												<div>
													<h3>{t(entry.titleKey)}</h3>
													<p>{t(entry.descriptionKey)}</p>
													<div className="skills-target-badges">
														{entry.skill.targetAgents.map((target) => (
															<span key={target}>
																{SKILL_TARGETS[target].name}
															</span>
														))}
													</div>
												</div>
												<Button
													variant="outline"
													size="sm"
													disabled={busy || !workspaceId}
													onClick={() =>
														setForm(catalogForm(entry.skill, installed))
													}
												>
													{t(
														installed
															? "skills.catalog.update"
															: "skills.design.usePreset",
													)}
													{!installed && <ChevronRight size={13} />}
												</Button>
											</article>
										);
									})}
								</div>
							) : section === "library" ? (
								<div className="skills-library">
									{filteredSkills.length === 0 ? (
										<div className="skills-empty">
											<FileText size={28} aria-hidden />
											<p>
												{search.trim()
													? t("skills.design.noResults")
													: t("skills.emptyList")}
											</p>
											{search.trim() && (
												<Button
													variant="ghost"
													size="sm"
													onClick={() => setSearch("")}
												>
													{t("skills.design.clearSearch")}
												</Button>
											)}
										</div>
									) : (
										<div className="skills-card-list">
											{filteredSkills.map((skill) => (
												<article key={skill.name} className="skills-card">
													<span className="skills-card-mark">
														<FileText size={17} aria-hidden />
													</span>
													<div className="skills-card-content">
														<div className="skills-card-heading">
															<h3>{skill.name}</h3>
															{skill.disableModelInvocation && (
																<Badge variant="outline">
																	{t("skills.slashOnlyBadge")}
																</Badge>
															)}
														</div>
											<p>{skill.description}</p>
											{skill.sourceUrl && (
											<a
												className="skills-origin-link"
												href={skill.sourceUrl}
												target="_blank"
												rel="noreferrer"
											>
												{t("skills.github.importedFrom")}
											</a>
										)}
										<div className="skills-target-badges">
															{skill.targetAgents.map((target) => (
																<span
																	key={target}
																	title={SKILL_TARGETS[target]?.path}
																>
																	{SKILL_TARGETS[target]?.name ?? target}
																</span>
															))}
														</div>
													</div>
													<div className="skills-card-actions">
														<Button
															size="icon-sm"
															variant="ghost"
															aria-label={t("skills.editAria", {
																name: skill.name,
															})}
															disabled={busy || !workspaceId}
															onClick={() =>
																setForm({
																	editingExisting: true,
																	name: skill.name,
																	description: skill.description,
															body: skill.body,
														sourceUrl: skill.sourceUrl,
																	targetAgents:
																		skill.targetAgents.length > 0
																			? skill.targetAgents
																			: ["claude"],
																	disableModelInvocation:
																		skill.disableModelInvocation,
																})
															}
														>
															<Pencil size={14} />
														</Button>
														<Button
															size="icon-sm"
															variant="ghost"
															className="skills-delete"
															aria-label={t("skills.deleteAria", {
																name: skill.name,
															})}
															disabled={busy || !workspaceId}
															onClick={() => void handleDelete(skill.name)}
														>
															<Trash2 size={14} />
														</Button>
													</div>
												</article>
											))}
										</div>
									)}
								</div>
							) : (
								<div className="skills-detections">
									<p className="skills-detection-hint">
										{t("skills.detected.hint")}
									</p>
									{detections.length === 0 ? (
										<div className="skills-empty">
											<FolderSearch size={28} aria-hidden />
											<p>{t("skills.detected.empty")}</p>
										</div>
									) : (
										detections.map((item) => (
											<article key={item.id} className="skills-detection">
												<FileText size={17} aria-hidden />
												<div>
													<div className="skills-detection-heading">
														<h3>{item.title}</h3>
														<Badge variant="outline">
															{t("skills.detected.itemCount", {
																count: item.count,
															})}
														</Badge>
														{item.managedCount > 0 && (
															<Badge variant="secondary">
																{t("skills.detected.managedBadge", {
																	count: item.managedCount,
																})}
															</Badge>
														)}
														{item.externalCount > 0 && (
															<Badge variant="outline">
																{t("skills.detected.externalBadge", {
																	count: item.externalCount,
																})}
															</Badge>
														)}
													</div>
													<code>{item.relativePath}</code>
													<span className="skills-root-kind">
														{item.rootKind === "project_root"
															? t("skills.detected.projectRoot")
															: t("skills.detected.worktreeRoot")}
													</span>
													<p>{detectionDescription(item)}</p>
												</div>
											</article>
										))
									)}
								</div>
							)}
						</div>
						<div className="skills-source">
							<span>{t("skills.design.source")}</span>
							<code>.devcommandcenter/skills/</code>
						</div>
					</>
				)}
			</DialogContent>
		</Dialog>
	);
}
