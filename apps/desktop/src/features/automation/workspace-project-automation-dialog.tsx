import { useQuery, useQueryClient } from "@tanstack/react-query";
import type {
	WorkspaceDeliveryPolicy,
	WorkspaceProjectTask,
	WorkspaceRunProjectTasksOutput,
} from "@dcc/contracts";
import {
	CheckCircle2,
	CircleAlert,
	Loader2,
	Play,
	Plus,
	Settings2,
	ShieldCheck,
	Trash2,
	Wrench,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
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
import {
	workspaceProjectAutomationConfig,
	workspaceRunProjectTasks,
	workspaceSaveProjectAutomation,
} from "@/lib/workspace-api";
import { cn } from "@/lib/utils";

export const WORKSPACE_AUTOMATION_QUERY_KEY = "workspaceProjectAutomation";

type Props = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	workspaceRoot: string | null;
	externalReport?: WorkspaceRunProjectTasksOutput | null;
	onWorkspaceChanged?: () => void;
	onConfigSaved?: () => void;
};

type Draft = {
	setupCommand: string;
	tasks: WorkspaceProjectTask[];
	beforeMerge: string[];
	beforePush: string[];
	deliveryPolicy: WorkspaceDeliveryPolicy;
};

const EMPTY_DRAFT: Draft = {
	setupCommand: "",
	tasks: [],
	beforeMerge: [],
	beforePush: [],
	deliveryPolicy: {
		minimumApprovals: 0,
		requirePipeline: false,
		requireResolvedDiscussions: false,
		requireCurrentBase: false,
		requireBeforeMergeChecks: false,
	},
};

function taskLabel(task: WorkspaceProjectTask) {
	return task.label?.trim() || task.id;
}

export function WorkspaceProjectAutomationDialog({
	open,
	onOpenChange,
	workspaceRoot,
	externalReport = null,
	onWorkspaceChanged,
	onConfigSaved,
}: Props) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const root = workspaceRoot?.trim() ?? "";
	const [draft, setDraft] = useState<Draft>(EMPTY_DRAFT);
	const [isSaving, setIsSaving] = useState(false);
	const [runningIds, setRunningIds] = useState<string[]>([]);
	const [result, setResult] = useState<WorkspaceRunProjectTasksOutput | null>(null);

	const configQuery = useQuery({
		queryKey: [WORKSPACE_AUTOMATION_QUERY_KEY, root],
		queryFn: () => workspaceProjectAutomationConfig({ workspaceRoot: root }),
		enabled: open && Boolean(root),
		refetchOnWindowFocus: false,
		staleTime: 0,
	});

	useEffect(() => {
		if (!open || !configQuery.data) return;
		setDraft({
			setupCommand: configQuery.data.setupCommand ?? "",
			tasks: configQuery.data.tasks,
			beforeMerge: configQuery.data.beforeMerge,
			beforePush: configQuery.data.beforePush,
			deliveryPolicy: configQuery.data.deliveryPolicy,
		});
	}, [configQuery.data, open]);

	useEffect(() => {
		if (open && externalReport) setResult(externalReport);
	}, [externalReport, open]);

	const checkTaskIds = useMemo(
		() => draft.tasks.filter((task) => task.kind === "check").map((task) => task.id),
		[draft.tasks],
	);
	const isDirty = useMemo(() => {
		if (!configQuery.data) return false;
		return (
			JSON.stringify(draft) !==
			JSON.stringify({
				setupCommand: configQuery.data.setupCommand ?? "",
				tasks: configQuery.data.tasks,
				beforeMerge: configQuery.data.beforeMerge,
				beforePush: configQuery.data.beforePush,
				deliveryPolicy: configQuery.data.deliveryPolicy,
			})
		);
	}, [configQuery.data, draft]);

	const updateTask = (index: number, patch: Partial<WorkspaceProjectTask>) => {
		setDraft((current) => {
			const previousId = current.tasks[index]?.id;
			const tasks = current.tasks.map((task, taskIndex) =>
				taskIndex === index ? { ...task, ...patch } : task,
			);
			const changed = tasks[index];
			const remapHook = (ids: string[]) =>
				previousId && changed?.id !== previousId
					? ids.map((id) => (id === previousId ? changed.id : id))
					: ids;
			const beforeMerge = remapHook(current.beforeMerge);
			const beforePush = remapHook(current.beforePush);
			return {
				...current,
				tasks,
				beforeMerge:
					changed?.kind === "fix"
						? beforeMerge.filter((id) => id !== changed.id)
						: beforeMerge,
				beforePush:
					changed?.kind === "fix"
						? beforePush.filter((id) => id !== changed.id)
						: beforePush,
			};
		});
	};

	const toggleHook = (hook: "beforeMerge" | "beforePush", id: string) => {
		setDraft((current) => ({
			...current,
			[hook]: current[hook].includes(id)
				? current[hook].filter((taskId) => taskId !== id)
				: [...current[hook], id],
		}));
	};

	const addTask = () => {
		setDraft((current) => {
			const used = new Set(current.tasks.map((task) => task.id));
			let suffix = current.tasks.length + 1;
			while (used.has(`check_${suffix}`)) suffix += 1;
			return {
				...current,
				tasks: [
					...current.tasks,
					{
						id: `check_${suffix}`,
						label: "",
						command: "",
						kind: "check",
						cwd: null,
						timeoutSeconds: 600,
					},
				],
			};
		});
	};

	const removeTask = (index: number) => {
		setDraft((current) => {
			const id = current.tasks[index]?.id;
			return {
				...current,
				tasks: current.tasks.filter((_, taskIndex) => taskIndex !== index),
				beforeMerge: current.beforeMerge.filter((taskId) => taskId !== id),
				beforePush: current.beforePush.filter((taskId) => taskId !== id),
			};
		});
	};

	const save = async () => {
		if (!root || !configQuery.data) return;
		setIsSaving(true);
		try {
			const saved = await workspaceSaveProjectAutomation({
				workspaceRoot: root,
				setupCommand: draft.setupCommand.trim() || null,
				tasks: draft.tasks.map((task) => ({
					...task,
					id: task.id.trim(),
					label: task.label?.trim() || null,
					command: task.command.trim(),
					cwd: task.cwd?.trim() || null,
				})),
				beforeMerge: [...new Set(draft.beforeMerge.map((id) => id.trim()))],
				beforePush: [...new Set(draft.beforePush.map((id) => id.trim()))],
				deliveryPolicy: draft.deliveryPolicy,
				expectedConfigHash: configQuery.data.configHash,
			});
			queryClient.setQueryData([WORKSPACE_AUTOMATION_QUERY_KEY, root], saved);
			onConfigSaved?.();
			toast.success(t("automation.saved"));
		} catch (error) {
			toast.error(t("automation.saveFailed"), { description: String(error) });
		} finally {
			setIsSaving(false);
		}
	};

	const runTasks = async (taskIds: string[]) => {
		if (!root || !configQuery.data || taskIds.length === 0) return;
		const selected = draft.tasks.filter((task) => taskIds.includes(task.id));
		if (
			selected.some((task) => task.kind === "fix") &&
			!window.confirm(t("automation.confirmFix"))
		) {
			return;
		}
		setRunningIds(taskIds);
		setResult(null);
		try {
			const output = await workspaceRunProjectTasks({
				workspaceRoot: root,
				taskIds,
				expectedConfigHash: configQuery.data.configHash,
			});
			setResult(output);
			if (output.report.status === "passed") {
				toast.success(t("automation.runPassed"));
			} else {
				toast.error(t("automation.runFailed"));
			}
			if (output.changedFiles) {
				onWorkspaceChanged?.();
				await queryClient.invalidateQueries({
					queryKey: [WORKSPACE_AUTOMATION_QUERY_KEY, root],
				});
			}
		} catch (error) {
			toast.error(t("automation.runFailed"), { description: String(error) });
		} finally {
			setRunningIds([]);
		}
	};

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="flex h-[min(88vh,920px)] max-w-[min(96vw,1080px)] grid-rows-none flex-col gap-0 overflow-hidden p-0 sm:max-w-[min(96vw,1080px)]">
				<DialogHeader className="shrink-0 border-b border-border/60 px-5 py-4 pr-12">
					<div className="flex items-center gap-2">
						<div className="flex size-8 items-center justify-center rounded-lg bg-primary/10 text-primary">
							<Settings2 className="size-4" />
						</div>
						<div>
							<DialogTitle>{t("automation.title")}</DialogTitle>
							<DialogDescription className="mt-1">
								{t("automation.description")}
							</DialogDescription>
						</div>
					</div>
				</DialogHeader>

				<div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
					{configQuery.isPending ? (
						<div className="flex h-full items-center justify-center text-muted-foreground">
							<Loader2 className="mr-2 size-4 animate-spin" />
							{t("automation.loading")}
						</div>
					) : configQuery.isError ? (
						<div className="rounded-xl border border-destructive/30 bg-destructive/5 p-4 text-destructive">
							{String(configQuery.error)}
						</div>
					) : (
						<div className="space-y-5">
							<section className="rounded-xl border border-border/60 bg-muted/15 p-4">
								<div className="mb-3">
									<h3 className="text-sm font-semibold">{t("automation.setupTitle")}</h3>
									<p className="mt-1 text-xs text-muted-foreground">
										{t("automation.setupDescription")}
									</p>
								</div>
								<Label htmlFor="automation-setup">{t("automation.command")}</Label>
								<Input
									id="automation-setup"
									className="mt-1.5 font-mono text-xs"
									value={draft.setupCommand}
									onChange={(event) =>
										setDraft((current) => ({ ...current, setupCommand: event.target.value }))
									}
									placeholder={t("automation.setupPlaceholder")}
								/>
							</section>

							<section className="rounded-xl border border-border/60 bg-muted/15 p-4">
								<div className="mb-3 flex items-start gap-2">
									<ShieldCheck className="mt-0.5 size-4 shrink-0 text-primary" />
									<div>
										<h3 className="text-sm font-semibold">
											{t("automation.deliveryTitle")}
										</h3>
										<p className="mt-1 text-xs leading-relaxed text-muted-foreground">
											{t("automation.deliveryDescription")}
										</p>
									</div>
								</div>
								<div className="grid gap-3 md:grid-cols-2">
									<div className="rounded-lg border border-border/50 bg-background/70 p-3">
										<Label htmlFor="delivery-minimum-approvals">
											{t("automation.minimumApprovals")}
										</Label>
										<Input
											id="delivery-minimum-approvals"
											className="mt-1.5"
											type="number"
											min={0}
											max={20}
											value={draft.deliveryPolicy.minimumApprovals}
											onChange={(event) =>
												setDraft((current) => ({
													...current,
													deliveryPolicy: {
														...current.deliveryPolicy,
														minimumApprovals: Math.min(
															20,
															Math.max(0, Number(event.target.value) || 0),
														),
													},
												}))
											}
										/>
										<p className="mt-1.5 text-[11px] leading-relaxed text-muted-foreground">
											{t("automation.minimumApprovalsHint")}
										</p>
									</div>
									<div className="space-y-2 rounded-lg border border-border/50 bg-background/70 p-3 text-xs">
										{(
											[
												["requirePipeline", "automation.requirePipeline"],
												[
													"requireResolvedDiscussions",
													"automation.requireResolvedDiscussions",
												],
												[
													"requireCurrentBase",
													"automation.requireCurrentBase",
												],
												[
													"requireBeforeMergeChecks",
													"automation.requireBeforeMergeChecks",
												],
											] as const
										).map(([key, label]) => (
											<label key={key} className="flex items-start gap-2">
												<input
													type="checkbox"
													className="mt-0.5"
													checked={draft.deliveryPolicy[key]}
													onChange={(event) =>
														setDraft((current) => ({
															...current,
															deliveryPolicy: {
																...current.deliveryPolicy,
																[key]: event.target.checked,
															},
														}))
													}
												/>
												<span>{t(label)}</span>
											</label>
										))}
									</div>
								</div>
								<p className="mt-3 text-[11px] leading-relaxed text-muted-foreground">
									{t("automation.deliveryPolicyHint")}
								</p>
							</section>

							<section>
								<div className="mb-3 flex flex-wrap items-center justify-between gap-2">
									<div>
										<h3 className="text-sm font-semibold">{t("automation.tasksTitle")}</h3>
										<p className="mt-1 text-xs text-muted-foreground">
											{t("automation.tasksDescription")}
										</p>
									</div>
									<div className="flex gap-2">
										<Button
											type="button"
											variant="outline"
											size="sm"
											disabled={checkTaskIds.length === 0 || runningIds.length > 0 || isDirty}
											title={isDirty ? t("automation.saveBeforeRun") : undefined}
											onClick={() => void runTasks(checkTaskIds)}
										>
											<Play className="size-3.5" />
											{t("automation.runChecks")}
										</Button>
										<Button type="button" size="sm" onClick={addTask}>
											<Plus className="size-3.5" />
											{t("automation.addTask")}
										</Button>
									</div>
								</div>

								{draft.tasks.length === 0 ? (
									<div className="rounded-xl border border-dashed border-border px-4 py-8 text-center text-sm text-muted-foreground">
										{t("automation.emptyTasks")}
									</div>
								) : (
									<div className="space-y-3">
										{draft.tasks.map((task, index) => (
											<div key={`${task.id}-${index}`} className="rounded-xl border border-border/60 bg-background p-4 shadow-sm">
												<div className="mb-3 flex items-center justify-between gap-2">
													<div className="flex min-w-0 items-center gap-2">
														{task.kind === "fix" ? <Wrench className="size-4 shrink-0 text-amber-600" /> : <CheckCircle2 className="size-4 shrink-0 text-emerald-600" />}
														<span className="truncate text-sm font-medium" title={taskLabel(task)}>{taskLabel(task)}</span>
														<Badge variant="outline" className="text-[10px]">{t(`automation.kind.${task.kind}`)}</Badge>
													</div>
													<div className="flex shrink-0 gap-1">
														<Button type="button" variant="ghost" size="icon-sm" disabled={runningIds.length > 0 || isDirty} title={isDirty ? t("automation.saveBeforeRun") : t("automation.runTask")} onClick={() => void runTasks([task.id])}>
															{runningIds.includes(task.id) ? <Loader2 className="animate-spin" /> : <Play />}
														</Button>
														<Button type="button" variant="ghost" size="icon-sm" title={t("automation.removeTask")} onClick={() => removeTask(index)}><Trash2 /></Button>
													</div>
												</div>
												<div className="grid gap-3 md:grid-cols-12">
													<div className="md:col-span-3"><Label>{t("automation.id")}</Label><Input className="mt-1 font-mono text-xs" value={task.id} onChange={(event) => updateTask(index, { id: event.target.value })} /></div>
													<div className="md:col-span-4"><Label>{t("automation.label")}</Label><Input className="mt-1" value={task.label ?? ""} onChange={(event) => updateTask(index, { label: event.target.value })} /></div>
													<div className="md:col-span-3"><Label>{t("automation.cwd")}</Label><Input className="mt-1 font-mono text-xs" value={task.cwd ?? ""} placeholder="." onChange={(event) => updateTask(index, { cwd: event.target.value || null })} /></div>
													<div className="md:col-span-2"><Label>{t("automation.kindLabel")}</Label><select className="mt-1 h-9 w-full rounded-md border border-input bg-transparent px-2 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring" value={task.kind} onChange={(event) => updateTask(index, { kind: event.target.value as WorkspaceProjectTask["kind"] })}><option value="check">{t("automation.kind.check")}</option><option value="fix">{t("automation.kind.fix")}</option></select></div>
													<div className="md:col-span-10"><Label>{t("automation.command")}</Label><Input className="mt-1 font-mono text-xs" value={task.command} onChange={(event) => updateTask(index, { command: event.target.value })} placeholder="yarn lint" /></div>
													<div className="md:col-span-2"><Label>{t("automation.timeout")}</Label><Input className="mt-1" type="number" min={1} max={3600} value={task.timeoutSeconds} onChange={(event) => updateTask(index, { timeoutSeconds: Number(event.target.value) })} /></div>
												</div>
												<div className="mt-3 flex flex-wrap gap-4 border-t border-border/50 pt-3 text-xs">
													<label className={cn("flex items-center gap-2", task.kind === "fix" && "cursor-not-allowed opacity-50")}><input type="checkbox" disabled={task.kind === "fix"} checked={draft.beforeMerge.includes(task.id)} onChange={() => toggleHook("beforeMerge", task.id)} />{t("automation.beforeMerge")}</label>
													<label className={cn("flex items-center gap-2", task.kind === "fix" && "cursor-not-allowed opacity-50")}><input type="checkbox" disabled={task.kind === "fix"} checked={draft.beforePush.includes(task.id)} onChange={() => toggleHook("beforePush", task.id)} />{t("automation.beforePush")}</label>
													{task.kind === "fix" ? <span className="text-muted-foreground">{t("automation.fixManualOnly")}</span> : null}
												</div>
											</div>
										))}
									</div>
								)}
							</section>

							{result ? (
								<section className={cn("rounded-xl border p-4", result.report.status === "passed" ? "border-emerald-500/30 bg-emerald-500/5" : "border-destructive/30 bg-destructive/5")}>
									<div className="mb-3 flex items-center gap-2">
										{result.report.status === "passed" ? <CheckCircle2 className="size-4 text-emerald-600" /> : <CircleAlert className="size-4 text-destructive" />}
										<h3 className="text-sm font-semibold">{result.report.status === "passed" ? t("automation.reportPassed") : t("automation.reportFailed")}</h3>
										{result.changedFiles ? <Badge variant="outline">{t("automation.filesChanged")}</Badge> : null}
									</div>
									<div className="space-y-2">
										{result.report.steps.map((step, index) => (
											<details key={`${step.command}-${index}`} open={!step.success} className="rounded-lg border border-border/60 bg-background/80 p-3">
												<summary className="cursor-pointer font-mono text-xs">{step.success ? "✓" : "✕"} {step.command}</summary>
												<pre className="mt-3 max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted/60 p-3 text-[11px] leading-5">{step.output || t("automation.noOutput")}</pre>
											</details>
										))}
									</div>
								</section>
							) : null}
						</div>
					)}
				</div>

				<DialogFooter className="mx-0 mb-0 shrink-0 rounded-none px-5 py-3">
					<div className="mr-auto hidden min-w-0 text-xs text-muted-foreground sm:block">
						<div className="flex min-w-0 items-center gap-2">
							<span className="block truncate" title={configQuery.data?.sourcePath}>{configQuery.data?.sourcePath}</span>
							{configQuery.data ? (
								<Badge variant={configQuery.data.trackedInGit ? "success" : "outline"} className="shrink-0 text-[10px]">
									{t(configQuery.data.trackedInGit ? "automation.trackedInGit" : "automation.notTrackedInGit")}
								</Badge>
							) : null}
						</div>
						<span>{t("automation.noPolling")}</span>
					</div>
					<Button type="button" variant="outline" onClick={() => onOpenChange(false)}>{t("automation.close")}</Button>
					<Button type="button" disabled={isSaving || configQuery.isPending || runningIds.length > 0} onClick={() => void save()}>{isSaving ? <Loader2 className="animate-spin" /> : null}{t("automation.save")}</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
