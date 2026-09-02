import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { CheckCircle2, LoaderCircle, Pause, Play, Target, Trash2 } from "lucide-react";
import type { ObjectiveTransition, SessionObjective } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Textarea } from "@/components/ui/textarea";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { composerToolbarTriggerClassName } from "@/features/composer/WorkspaceComposer.logic";
import { cn } from "@/lib/utils";
import {
	clearSessionObjective,
	getSessionObjective,
	setSessionObjective,
	transitionSessionObjective,
} from "@/lib/session-api";
import {
	EMPTY_OBJECTIVE_FORM,
	availableObjectiveTransitions,
	objectiveDraftFromForm,
	objectiveFormFromRecord,
	summarizeObjective,
	type ObjectiveFormDraft,
} from "./session-objective.logic";

export const SESSION_OBJECTIVE_QUERY_KEY = "session-objective";

/**
 * Durable task objective for the active conversation. The backend owns the
 * record and re-sends it with every turn; this control only lets the person
 * read, edit, pause, resume, complete or clear it. Counters refresh when the
 * session's turn count or state changes, never by polling.
 */
export function SessionObjectiveControl({
	sessionId,
	refreshKey,
	disabled,
}: {
	sessionId: string | null;
	/** Changes whenever a turn starts or ends so counters stay fresh. */
	refreshKey: string;
	disabled: boolean;
}) {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [open, setOpen] = useState(false);
	const [form, setForm] = useState<ObjectiveFormDraft>(EMPTY_OBJECTIVE_FORM);
	const [dirty, setDirty] = useState(false);

	const query = useQuery({
		queryKey: [SESSION_OBJECTIVE_QUERY_KEY, sessionId],
		queryFn: async () => (sessionId ? (await getSessionObjective(sessionId)).objective : null),
		enabled: Boolean(sessionId),
		staleTime: 5_000,
		refetchOnWindowFocus: false,
		retry: false,
	});
	useEffect(() => {
		if (!sessionId) return;
		void queryClient.invalidateQueries({
			queryKey: [SESSION_OBJECTIVE_QUERY_KEY, sessionId],
		});
	}, [queryClient, refreshKey, sessionId]);

	const objective = query.data ?? null;
	useEffect(() => {
		if (!dirty) setForm(objectiveFormFromRecord(objective));
	}, [dirty, objective]);
	// An automatic pause (budget or failure limit) must be visible even when
	// the popover is closed: it is the moment the person has to decide.
	const previousStatusRef = useRef<{ sessionId: string | null; status: string | null }>({
		sessionId: null,
		status: null,
	});
	useEffect(() => {
		const previous = previousStatusRef.current;
		const status = objective?.status ?? null;
		if (
			previous.sessionId === sessionId &&
			previous.status === "active" &&
			status === "paused" &&
			objective?.pauseReason &&
			objective.pauseReason !== "manual"
		) {
			toast.warning(t("composer.objective.autoPaused"), {
				description: t(`composer.objective.pauseReason.${objective.pauseReason}`),
			});
		}
		previousStatusRef.current = { sessionId, status };
	}, [objective, sessionId, t]);
	useEffect(() => {
		setDirty(false);
		setForm(EMPTY_OBJECTIVE_FORM);
	}, [sessionId]);

	const settle = (next: SessionObjective | null) => {
		queryClient.setQueryData([SESSION_OBJECTIVE_QUERY_KEY, sessionId], next);
		setDirty(false);
	};
	const failure = (error: unknown) =>
		toast.error(t("composer.objective.failed"), {
			description: error instanceof Error ? error.message : String(error),
		});

	const save = useMutation({
		mutationFn: async (draftForm: ObjectiveFormDraft) => {
			if (!sessionId) throw new Error("no session");
			const parsed = objectiveDraftFromForm(draftForm);
			if (parsed.error) throw new Error(t(`composer.objective.errors.${parsed.error}`));
			return (
				await setSessionObjective({
					sessionId,
					draft: parsed.draft,
					expectedGeneration: objective?.generation ?? null,
				})
			).objective;
		},
		onSuccess: (next) => {
			settle(next);
			toast.success(t("composer.objective.saved"));
		},
		onError: failure,
	});
	const transition = useMutation({
		mutationFn: async (kind: ObjectiveTransition) => {
			if (!sessionId || !objective) throw new Error("no objective");
			return (
				await transitionSessionObjective({
					sessionId,
					transition: kind,
					expectedGeneration: objective.generation,
				})
			).objective;
		},
		onSuccess: settle,
		onError: failure,
	});
	const clear = useMutation({
		mutationFn: async () => {
			if (!sessionId) throw new Error("no session");
			await clearSessionObjective(sessionId);
			return null;
		},
		onSuccess: () => {
			settle(null);
			setOpen(false);
		},
		onError: failure,
	});

	const summary = useMemo(() => (objective ? summarizeObjective(objective) : null), [objective]);
	const busy = save.isPending || transition.isPending || clear.isPending;
	if (!sessionId) return null;

	const statusTone =
		summary?.status === "active"
			? "text-emerald-600 dark:text-emerald-400"
			: summary?.status === "paused"
				? "text-amber-600 dark:text-amber-400"
				: "text-muted-foreground";

	return (
		<Popover open={open} onOpenChange={setOpen}>
			<Tooltip>
				<TooltipTrigger asChild>
					<PopoverTrigger asChild>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							className={cn(
								composerToolbarTriggerClassName,
								"inline-flex h-7 items-center gap-1 rounded-[9px] px-1.5 text-[var(--dcc-daily-meta-size)]",
								objective
									? "bg-accent/60 text-foreground"
									: "text-muted-foreground/70 hover:text-muted-foreground/70",
							)}
							disabled={disabled}
							aria-label={t("composer.objective.open")}
							aria-busy={query.isLoading}
							data-testid="session-objective-control"
						>
							{query.isLoading && !objective ? (
								<LoaderCircle className="size-[13px] shrink-0 animate-spin" strokeWidth={1.8} />
							) : (
								<Target
									className={cn("size-[13px] shrink-0", statusTone)}
									strokeWidth={1.8}
								/>
							)}
							<span className="dcc-composer-objective-label text-[12px] font-medium leading-4">
								{t("composer.objective.compact")}
							</span>
						</Button>
					</PopoverTrigger>
				</TooltipTrigger>
				<TooltipContent side="top" className="max-w-80 flex-col items-start gap-0">
					<p className="font-medium">{t("composer.objective.tooltipTitle")}</p>
					<p className="mt-1 text-[11px] leading-4 text-background/75">
						{t("composer.objective.tooltipDescription")}
					</p>
				</TooltipContent>
			</Tooltip>
			<PopoverContent side="top" align="start" className="w-[26rem] max-w-[calc(100vw-1rem)] p-3">
				<div className="mb-2 flex items-center justify-between gap-2">
					<p className="text-[12px] font-medium">{t("composer.objective.title")}</p>
					{objective ? (
						<span className="text-[10px] text-muted-foreground">
							{t("composer.objective.generation", { generation: objective.generation })}
						</span>
					) : null}
				</div>
				<p className="mb-2 text-[11px] leading-4 text-muted-foreground">
					{t("composer.objective.hint")}
				</p>
				<div className="space-y-2">
					<div className="space-y-1">
						<Label htmlFor="objective-intent" className="text-[11px]">
							{t("composer.objective.intent")}
						</Label>
						<Textarea
							id="objective-intent"
							value={form.intent}
							onChange={(event) => {
								setDirty(true);
								setForm((current) => ({ ...current, intent: event.target.value }));
							}}
							placeholder={t("composer.objective.intentPlaceholder")}
							className="min-h-[56px] text-[12px]"
							disabled={busy}
						/>
					</div>
					<div className="space-y-1">
						<Label htmlFor="objective-done-when" className="text-[11px]">
							{t("composer.objective.doneWhen")}
						</Label>
						<Textarea
							id="objective-done-when"
							value={form.doneWhen}
							onChange={(event) => {
								setDirty(true);
								setForm((current) => ({ ...current, doneWhen: event.target.value }));
							}}
							placeholder={t("composer.objective.doneWhenPlaceholder")}
							className="min-h-[48px] text-[12px]"
							disabled={busy}
						/>
					</div>
					<div className="grid grid-cols-2 gap-2">
						<div className="space-y-1">
							<Label htmlFor="objective-max-failures" className="text-[11px]">
								{t("composer.objective.maxFailures")}
							</Label>
							<Input
								id="objective-max-failures"
								inputMode="numeric"
								value={form.maxConsecutiveFailures}
								onChange={(event) => {
									setDirty(true);
									setForm((current) => ({
										...current,
										maxConsecutiveFailures: event.target.value,
									}));
								}}
								className="h-8 text-[12px]"
								disabled={busy}
							/>
						</div>
						<div className="space-y-1">
							<Label htmlFor="objective-max-turns" className="text-[11px]">
								{t("composer.objective.maxTurns")}
							</Label>
							<Input
								id="objective-max-turns"
								inputMode="numeric"
								value={form.maxTurns}
								onChange={(event) => {
									setDirty(true);
									setForm((current) => ({ ...current, maxTurns: event.target.value }));
								}}
								placeholder={t("composer.objective.unlimited")}
								className="h-8 text-[12px]"
								disabled={busy}
							/>
						</div>
					</div>
				</div>
				<div className="mt-3 flex flex-wrap items-center gap-1.5">
					<Button
						type="button"
						size="xs"
						disabled={busy || !dirty}
						onClick={() => save.mutate(form)}
					>
						{save.isPending ? <LoaderCircle className="size-3 animate-spin" /> : null}
						{objective ? t("composer.objective.save") : t("composer.objective.create")}
					</Button>
					{objective
						? availableObjectiveTransitions(objective).map((kind) => (
								<Button
									key={kind}
									type="button"
									variant="outline"
									size="xs"
									disabled={busy}
									onClick={() => transition.mutate(kind)}
								>
									{kind === "pause" ? (
										<Pause className="size-3" />
									) : kind === "resume" ? (
										<Play className="size-3" />
									) : (
										<CheckCircle2 className="size-3" />
									)}
									{t(`composer.objective.transition.${kind}`)}
								</Button>
							))
						: null}
					{objective ? (
						<Button
							type="button"
							variant="ghost"
							size="xs"
							className="ml-auto text-muted-foreground hover:text-destructive"
							disabled={busy}
							onClick={() => clear.mutate()}
						>
							<Trash2 className="size-3" />
							{t("composer.objective.clear")}
						</Button>
					) : null}
				</div>
				{summary?.blocksAutomaticDispatch ? (
					<p className="mt-2 text-[10.5px] leading-4 text-amber-700 dark:text-amber-400">
						{t("composer.objective.dispatchBlocked")}
					</p>
				) : null}
			</PopoverContent>
		</Popover>
	);
}
