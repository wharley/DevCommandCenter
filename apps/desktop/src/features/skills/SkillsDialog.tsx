import { Pencil, Plus, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
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
	listSkills,
	saveSkill,
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
	const [skills, setSkills] = useState<SkillRecord[]>([]);
	const [loading, setLoading] = useState(false);
	const [form, setForm] = useState<FormState | null>(null);
	const [busy, setBusy] = useState(false);

	const refresh = useCallback(async () => {
		if (!projectRoot) {
			setSkills([]);
			return;
		}
		setLoading(true);
		try {
			setSkills(await listSkills(projectRoot));
		} catch (error) {
			toast.error(error instanceof Error ? error.message : "Failed to load skills");
		} finally {
			setLoading(false);
		}
	}, [projectRoot]);

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
				error instanceof Error ? error.message : "Failed to compile skills",
			);
		}
	}, [projectRoot, targetRoot]);

	const handleSave = useCallback(async () => {
		if (!projectRoot || !form) {
			return;
		}
		const name = form.name.trim();
		if (!NAME_PATTERN.test(name)) {
			toast.error("Name must be lower-case letters, digits and hyphens (e.g. review-pr).");
			return;
		}
		if (!form.description.trim()) {
			toast.error("A description is required — it is what the model sees in the catalog.");
			return;
		}
		if (form.targetAgents.length === 0) {
			toast.error("Pick at least one target agent.");
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
			toast.success(`Skill "${name}" saved`);
			setForm(null);
			await refresh();
		} catch (error) {
			toast.error(error instanceof Error ? error.message : "Failed to save skill");
		} finally {
			setBusy(false);
		}
	}, [projectRoot, form, recompile, refresh]);

	const handleDelete = useCallback(
		async (name: string) => {
			if (!projectRoot) {
				return;
			}
			setBusy(true);
			try {
				await deleteSkill(projectRoot, name);
				await recompile();
				toast.success(`Skill "${name}" deleted`);
				await refresh();
			} catch (error) {
				toast.error(error instanceof Error ? error.message : "Failed to delete skill");
			} finally {
				setBusy(false);
			}
		},
		[projectRoot, recompile, refresh],
	);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-w-2xl">
				<DialogHeader>
					<DialogTitle>Skills</DialogTitle>
					<DialogDescription>
						Write once in a neutral source; DCC compiles to each agent. Source:
						<code className="ml-1 rounded bg-muted px-1 py-0.5 text-[11px]">
							.devcommandcenter/skills/
						</code>
					</DialogDescription>
				</DialogHeader>

				{!projectRoot ? (
					<p className="py-8 text-center text-sm text-muted-foreground">
						Select a project to manage its skills.
					</p>
				) : form ? (
					<div className="flex flex-col gap-4">
						<div className="grid grid-cols-2 gap-3">
							<div className="flex flex-col gap-1.5">
								<Label htmlFor="skill-name">Name</Label>
								<Input
									id="skill-name"
									placeholder="review-pr"
									value={form.name}
									disabled={form.editingExisting}
									onChange={(e) => setForm({ ...form, name: e.target.value })}
								/>
							</div>
							<div className="flex flex-col gap-1.5">
								<Label htmlFor="skill-desc">Description</Label>
								<Input
									id="skill-desc"
									placeholder="When to use this skill"
									value={form.description}
									onChange={(e) => setForm({ ...form, description: e.target.value })}
								/>
							</div>
						</div>

						<div className="flex flex-col gap-1.5">
							<Label htmlFor="skill-body">Skill content</Label>
							<Textarea
								id="skill-body"
								className="min-h-[160px] font-mono text-[12px]"
								placeholder="Instructions, steps, examples…"
								value={form.body}
								onChange={(e) => setForm({ ...form, body: e.target.value })}
							/>
						</div>

						<div className="flex flex-col gap-1.5">
							<Label>Target agents</Label>
							<ToggleGroup
								type="multiple"
								value={form.targetAgents}
								onValueChange={(value) =>
									setForm({ ...form, targetAgents: value as SkillTargetAgent[] })
								}
								className="justify-start"
							>
								<ToggleGroupItem value="claude" className="border border-border/60 px-2.5 py-1">
									{TARGET_LABELS.claude}
								</ToggleGroupItem>
								<ToggleGroupItem value="agents" className="border border-border/60 px-2.5 py-1">
									{TARGET_LABELS.agents}
								</ToggleGroupItem>
							</ToggleGroup>
							<p className="text-[11px] text-muted-foreground">
								Claude gets a faithful copy (progressive disclosure). AGENTS.md is
								always-on instruction — keep few skills enabled there.
							</p>
						</div>

						<div className="flex items-center justify-between rounded-md border border-border/60 px-3 py-2">
							<div className="pr-3">
								<p className="text-[13px] font-medium">Disable model invocation</p>
								<p className="text-[11px] text-muted-foreground">
									Hide from the model's auto-catalog (excluded from AGENTS.md). Still
									available as a slash command.
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
								Cancel
							</Button>
							<Button onClick={() => void handleSave()} disabled={busy}>
								Save skill
							</Button>
						</DialogFooter>
					</div>
				) : (
					<div className="flex flex-col gap-3">
						<div className="flex items-center justify-between">
							<p className="text-[12px] text-muted-foreground">
								{loading
									? "Loading…"
									: `${skills.length} skill${skills.length === 1 ? "" : "s"}`}
							</p>
							<Button size="sm" onClick={() => setForm(emptyForm())}>
								<Plus className="size-4" /> New skill
							</Button>
						</div>

						<ScrollArea className="max-h-[320px]">
							<div className="flex flex-col gap-2 pr-3">
								{skills.length === 0 && !loading ? (
									<p className="py-8 text-center text-sm text-muted-foreground">
										No skills yet. Create one — it will be compiled for the agents you pick.
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
														slash-only
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
												aria-label={`Edit ${skill.name}`}
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
												aria-label={`Delete ${skill.name}`}
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
						{targetRoot ? (
							<p className="text-[11px] text-muted-foreground">
								Compiles into the active worktree so the running agent sees changes.
							</p>
						) : null}
					</div>
				)}
			</DialogContent>
		</Dialog>
	);
}
