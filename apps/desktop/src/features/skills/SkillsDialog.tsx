import { FileText, FolderSearch, Pencil, Plus, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
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
import { ScrollArea } from "@/components/ui/scroll-area";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
	compileSkills,
	deleteSkill,
	detectSkillContext,
	listSkills,
	saveSkill,
	type SkillContextDetection,
	type SkillRecord,
	type SkillTargetAgent,
} from "@/lib/skills-api";

export type SkillsDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	/** Project root (`rootPath`) — the source of truth `.devcommandcenter/skills/` lives here. */
	projectRoot: string | null;
	/** Active worktree path where compiled artifacts (`.claude/skills`, `AGENTS.md`) land. */
	targetRoot: string | null;
};

const TARGET_LABELS: Record<SkillTargetAgent, string> = {
	claude: "Claude (.claude/skills)",
	agents: "Codex · Droid (AGENTS.md)",
	gemini: "Gemini (GEMINI.md)",
	cursor: "Cursor (.cursor/rules)",
};

const NAME_PATTERN = /^[a-z0-9][a-z0-9-]*$/;

type FormState = {
	editingExisting: boolean;
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

export function SkillsDialog({
	open,
	onOpenChange,
	projectRoot,
	targetRoot,
}: SkillsDialogProps) {
	const { t } = useTranslation("common");
	const [skills, setSkills] = useState<SkillRecord[]>([]);
	const [detections, setDetections] = useState<SkillContextDetection[]>([]);
	const [loading, setLoading] = useState(false);
	const [form, setForm] = useState<FormState | null>(null);
	const [busy, setBusy] = useState(false);

	const refresh = useCallback(async () => {
		if (!projectRoot) {
			setSkills([]);
			setDetections([]);
			return;
		}
		setLoading(true);
		try {
			const [nextSkills, nextDetections] = await Promise.all([
				listSkills(projectRoot),
				detectSkillContext(projectRoot, targetRoot),
			]);
			setSkills(nextSkills);
			setDetections(nextDetections);
		} catch (error) {
			toast.error(error instanceof Error ? error.message : t("skills.toast.loadError"));
		} finally {
			setLoading(false);
		}
	}, [projectRoot, targetRoot, t]);

	useEffect(() => {
		if (open) {
			setForm(null);
			void refresh();
		}
	}, [open, refresh]);

	const recompile = useCallback(async () => {
		if (!projectRoot || !targetRoot) {
			return;
		}
		try {
			await compileSkills(projectRoot, targetRoot);
		} catch (error) {
			toast.error(
				error instanceof Error ? error.message : t("skills.toast.compileError"),
			);
		}
	}, [projectRoot, targetRoot, t]);

	const handleSave = useCallback(async () => {
		if (!projectRoot || !form) {
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
			await saveSkill(projectRoot, {
				name,
				description: form.description.trim(),
				body: form.body,
				targetAgents: form.targetAgents,
				disableModelInvocation: form.disableModelInvocation,
				scope: "project",
			});
			await recompile();
			toast.success(t("skills.toast.saved", { name }));
			setForm(null);
			await refresh();
		} catch (error) {
			toast.error(error instanceof Error ? error.message : t("skills.toast.saveError"));
		} finally {
			setBusy(false);
		}
	}, [projectRoot, form, recompile, refresh, t]);

	const handleDelete = useCallback(
		async (name: string) => {
			if (!projectRoot) {
				return;
			}
			setBusy(true);
			try {
				await deleteSkill(projectRoot, name);
				await recompile();
				toast.success(t("skills.toast.deleted", { name }));
				await refresh();
			} catch (error) {
				toast.error(error instanceof Error ? error.message : t("skills.toast.deleteError"));
			} finally {
				setBusy(false);
			}
		},
		[projectRoot, recompile, refresh, t],
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
				default:
					return t("skills.detected.behavior.generic");
			}
		},
		[t],
	);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="w-full sm:max-w-2xl">
				<DialogHeader>
					<DialogTitle>{t("skills.title")}</DialogTitle>
					<DialogDescription>
						{t("skills.description")}
						<code className="ml-1 rounded bg-muted px-1 py-0.5 text-[11px]">
							.devcommandcenter/skills/
						</code>
					</DialogDescription>
				</DialogHeader>

				{!projectRoot ? (
					<p className="py-8 text-center text-sm text-muted-foreground">
						{t("skills.selectProject")}
					</p>
				) : form ? (
					<div className="flex flex-col gap-4">
						<div className="grid grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-3">
							<div className="flex min-w-0 flex-col gap-1.5">
								<Label htmlFor="skill-name">{t("skills.fields.name")}</Label>
								<Input
									id="skill-name"
									placeholder={t("skills.fields.namePlaceholder")}
									value={form.name}
									disabled={form.editingExisting}
									onChange={(e) => setForm({ ...form, name: e.target.value })}
								/>
							</div>
							<div className="flex min-w-0 flex-col gap-1.5">
								<Label htmlFor="skill-desc">{t("skills.fields.description")}</Label>
								<Input
									id="skill-desc"
									placeholder={t("skills.fields.descriptionPlaceholder")}
									value={form.description}
									onChange={(e) => setForm({ ...form, description: e.target.value })}
								/>
							</div>
						</div>

						<div className="flex flex-col gap-1.5">
							<Label htmlFor="skill-body">{t("skills.fields.body")}</Label>
							<Textarea
								id="skill-body"
								className="min-h-[160px] font-mono text-[12px]"
								placeholder={t("skills.fields.bodyPlaceholder")}
								value={form.body}
								onChange={(e) => setForm({ ...form, body: e.target.value })}
							/>
						</div>

						<div className="flex flex-col gap-1.5">
							<Label>{t("skills.fields.targetAgents")}</Label>
							<ToggleGroup
								type="multiple"
								value={form.targetAgents}
								onValueChange={(value) =>
									setForm({ ...form, targetAgents: value as SkillTargetAgent[] })
								}
								className="grid grid-cols-2 gap-2"
							>
								<ToggleGroupItem
									value="claude"
									className="w-full justify-start border border-border/60 px-3 py-2"
								>
									{TARGET_LABELS.claude}
								</ToggleGroupItem>
								<ToggleGroupItem
									value="agents"
									className="w-full justify-start border border-border/60 px-3 py-2"
								>
									{TARGET_LABELS.agents}
								</ToggleGroupItem>
								<ToggleGroupItem
									value="gemini"
									className="w-full justify-start border border-border/60 px-3 py-2"
								>
									{TARGET_LABELS.gemini}
								</ToggleGroupItem>
								<ToggleGroupItem
									value="cursor"
									className="w-full justify-start border border-border/60 px-3 py-2"
								>
									{TARGET_LABELS.cursor}
								</ToggleGroupItem>
							</ToggleGroup>
							<p className="text-[11px] text-muted-foreground">
								{t("skills.targetsHint")}
							</p>
						</div>

						<div className="flex items-center justify-between rounded-md border border-border/60 px-3 py-2">
							<div className="pr-3">
								<p className="text-[13px] font-medium">
									{t("skills.disableInvocation.title")}
								</p>
								<p className="text-[11px] text-muted-foreground">
									{t("skills.disableInvocation.body")}
								</p>
							</div>
							<Switch
								checked={form.disableModelInvocation}
								onCheckedChange={(checked) =>
									setForm({ ...form, disableModelInvocation: checked })
								}
							/>
						</div>

						<DialogFooter>
							<Button variant="ghost" onClick={() => setForm(null)} disabled={busy}>
								{t("skills.cancel")}
							</Button>
							<Button onClick={() => void handleSave()} disabled={busy}>
								{t("skills.save")}
							</Button>
						</DialogFooter>
					</div>
				) : (
					<div className="flex flex-col gap-3">
						<div className="flex items-center justify-between">
							<p className="text-[12px] text-muted-foreground">
								{loading
									? t("skills.loading")
									: t("skills.count", { count: skills.length })}
							</p>
							<Button size="sm" onClick={() => setForm(emptyForm())}>
								<Plus className="size-4" /> {t("skills.newSkill")}
							</Button>
						</div>

						<ScrollArea className="max-h-[320px]">
							<div className="flex flex-col gap-2 pr-3">
								{skills.length === 0 && !loading ? (
									<p className="py-8 text-center text-sm text-muted-foreground">
										{t("skills.emptyList")}
									</p>
								) : null}
								{skills.map((skill) => (
									<div
										key={skill.name}
										className="flex items-start justify-between gap-3 rounded-md border border-border/60 px-3 py-2"
									>
										<div className="min-w-0">
											<div className="flex items-center gap-2">
												<span className="truncate text-[13px] font-medium">
													{skill.name}
												</span>
												{skill.disableModelInvocation ? (
													<Badge variant="outline" className="text-[10px]">
														{t("skills.slashOnlyBadge")}
													</Badge>
												) : null}
											</div>
											<p className="truncate text-[12px] text-muted-foreground">
												{skill.description}
											</p>
											<div className="mt-1 flex flex-wrap gap-1">
												{skill.targetAgents.map((target) => (
													<Badge key={target} variant="secondary" className="text-[10px]">
														{TARGET_LABELS[target] ?? target}
													</Badge>
												))}
											</div>
										</div>
										<div className="flex shrink-0 gap-1">
											<Button
												size="icon"
												variant="ghost"
												aria-label={t("skills.editAria", { name: skill.name })}
												disabled={busy}
												onClick={() =>
													setForm({
														editingExisting: true,
														name: skill.name,
														description: skill.description,
														body: skill.body,
														targetAgents:
															skill.targetAgents.length > 0
																? skill.targetAgents
																: ["claude"],
														disableModelInvocation: skill.disableModelInvocation,
													})
												}
											>
												<Pencil className="size-4" />
											</Button>
											<Button
												size="icon"
												variant="ghost"
												aria-label={t("skills.deleteAria", { name: skill.name })}
												disabled={busy}
												onClick={() => void handleDelete(skill.name)}
											>
												<Trash2 className="size-4" />
											</Button>
										</div>
									</div>
								))}
							</div>
						</ScrollArea>

						<div className="rounded-md border border-border/60 px-3 py-2">
							<div className="mb-2 flex items-start justify-between gap-3">
								<div className="min-w-0">
									<p className="text-[13px] font-medium">
										{t("skills.detected.title")}
									</p>
									<p className="text-[11px] text-muted-foreground">
										{t("skills.detected.hint")}
									</p>
								</div>
								<FolderSearch className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
							</div>
							{detections.length === 0 && !loading ? (
								<p className="py-3 text-center text-[12px] text-muted-foreground">
									{t("skills.detected.empty")}
								</p>
							) : (
								<div className="flex flex-col gap-2">
									{detections.map((item) => (
										<div
											key={item.id}
											className="flex min-w-0 items-start gap-2 rounded-md bg-muted/35 px-2.5 py-2"
										>
											<FileText className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
											<div className="min-w-0 flex-1">
												<div className="flex min-w-0 flex-wrap items-center gap-1.5">
													<span className="truncate text-[12px] font-medium">
														{item.title}
													</span>
													<Badge variant="outline" className="text-[10px]">
														{t("skills.detected.itemCount", { count: item.count })}
													</Badge>
													{item.managedCount > 0 ? (
														<Badge variant="secondary" className="text-[10px]">
															{t("skills.detected.managedBadge", {
																count: item.managedCount,
															})}
														</Badge>
													) : null}
													{item.externalCount > 0 ? (
														<Badge variant="outline" className="text-[10px]">
															{t("skills.detected.externalBadge", {
																count: item.externalCount,
															})}
														</Badge>
													) : null}
												</div>
												<p className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">
													{item.relativePath}
													{" · "}
													{item.rootKind === "project_root"
														? t("skills.detected.projectRoot")
														: t("skills.detected.worktreeRoot")}
												</p>
												<p className="mt-1 text-[11px] text-muted-foreground">
													{detectionDescription(item)}
												</p>
											</div>
										</div>
									))}
								</div>
							)}
						</div>
						{targetRoot ? (
							<p className="text-[11px] text-muted-foreground">
								{t("skills.compileNote")}
							</p>
						) : null}
					</div>
				)}
			</DialogContent>
		</Dialog>
	);
}
