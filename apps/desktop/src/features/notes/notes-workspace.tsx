import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import {
	ArrowUpRight,
	Check,
	CheckCheck,
	Circle,
	Lightbulb,
	Loader2,
	MessageCircle,
	Pin,
	Plus,
	Search,
	StickyNote,
	Trash2,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { notePrompt, type ProjectNote } from "./notes-api";
import type { useProjectNotes } from "./use-project-notes";
import { FloatingNote, type FloatingState } from "./floating-note";
import {
	defaultNotePosition,
	NOTE_POSITION_KEY,
	readNotePositions,
	restoreOpenTaskNotes,
} from "./restore-note-balloons";
import "./notes.css";

export type NotesScope = {
	projectId: string;
	projectName: string;
	workspaceId: string | null;
	sessionId: string | null;
	taskTitle: string;
};
type Project = { id: string; name: string };
type Props = {
	controller: ReturnType<typeof useProjectNotes>;
	open: boolean;
	onOpenChange: (open: boolean) => void;
	scope: NotesScope | null;
	projects: Project[];
	onUse: (text: string) => void;
	onCreateTask: (note: ProjectNote) => Promise<string>;
	completionTaskIds: string[];
	onCompletionHandled: () => void;
};

export function NotesWorkspace({
	controller,
	open,
	onOpenChange,
	scope,
	projects,
	onUse,
	onCreateTask,
	completionTaskIds,
	onCompletionHandled,
}: Props) {
	const { t } = useTranslation("common");
	const reduced = useReducedMotion();
	const [projectFilter, setProjectFilter] = useState("all");
	const [conversationFilter, setConversationFilter] = useState("all");
	const [statusFilter, setStatusFilter] = useState<"open" | "completed">(
		"open",
	);
	const [search, setSearch] = useState("");
	const [floating, setFloating] = useState<Record<string, FloatingState>>({});
	const [focusNoteId, setFocusNoteId] = useState<string | null>(null);
	const [deleteIds, setDeleteIds] = useState<string[]>([]);
	const [busy, setBusy] = useState(false);
	const [creating, setCreating] = useState(false);
	const creatingRef = useRef(false);
	const scopeRef = useRef(scope);
	scopeRef.current = scope;
	const [savedPositions] = useState(readNotePositions);
	const positionsRef = useRef(savedPositions);
	const lastScope = useRef<string | null>(null);
	const dismissed = useRef(new Set<string>());
	useEffect(() => {
		if (controller.loading) return;
		const scopeKey = JSON.stringify([scope?.projectId, scope?.workspaceId]);
		const changedScope = lastScope.current !== scopeKey;
		lastScope.current = scopeKey;
		if (changedScope) {
			dismissed.current.clear();
			setFocusNoteId(null);
		}
		setFloating((current) => {
			positionsRef.current = { ...positionsRef.current, ...current };
			return restoreOpenTaskNotes({
				notes: controller.notes,
				current,
				saved: positionsRef.current,
				dismissed: dismissed.current,
				scope: scopeRef.current,
				changedScope,
			});
		});
	}, [
		controller.notes,
		controller.loading,
		scope?.projectId,
		scope?.workspaceId,
	]);
	const allProjects = useMemo(
		() => [
			...new Map(
				[
					...controller.notes.map((note) => ({
						id: note.projectId,
						name: note.projectName,
					})),
					...projects,
				].map((project) => [project.id, project]),
			).values(),
		],
		[controller.notes, projects],
	);
	const previousOpen = useRef(false);
	useEffect(() => {
		if (open && !previousOpen.current) {
			setProjectFilter(scopeRef.current?.projectId ?? "all");
			setConversationFilter("all");
			void controller.store.load();
		}
		previousOpen.current = open;
	}, [open, controller.store]);
	const show = useCallback(
		(note: ProjectNote) => {
			setFocusNoteId(note.id);
			dismissed.current.delete(note.id);
			setFloating((current) => {
				const others = Object.entries(current)
					.filter(([id]) => id !== note.id)
					.slice(-3);
				const index = others.length;
				positionsRef.current = { ...positionsRef.current, ...current };
				const position =
					positionsRef.current[note.id] ?? defaultNotePosition(index);
				return {
					...Object.fromEntries(others),
					[note.id]: { ...position, minimized: false },
				};
			});
			onOpenChange(false);
		},
		[onOpenChange],
	);
	const close = (id: string) => {
		dismissed.current.add(id);
		setFloating((current) => {
			positionsRef.current = { ...positionsRef.current, ...current };
			const next = { ...current };
			delete next[id];
			return next;
		});
	};
	const create = useCallback(
		async (context = "") => {
			if (creatingRef.current) return;
			const capturedScope = scopeRef.current;
			const project = projects.find(
				(project) =>
					project.id ===
					(open && projectFilter !== "all"
						? projectFilter
						: capturedScope?.projectId),
			);
			if (!project) {
				onOpenChange(true);
				toast.info(t("notes.chooseProject"));
				return;
			}
			setCreating(true);
			creatingRef.current = true;
			try {
				const sameProject = capturedScope?.projectId === project.id;
				if ([...context].length > 12_000) {
					toast.error(t("notes.contextTooLong"));
					return;
				}
				const note = await controller.store.create({
					projectId: project.id,
					projectName: project.name,
					title: "",
					content: "",
					contextSnapshot: sameProject ? context : "",
					sourceSessionId: sameProject ? capturedScope.sessionId : null,
					sourceWorkspaceId: sameProject ? capturedScope.workspaceId : null,
					sourceTaskTitle: sameProject
						? capturedScope.taskTitle.slice(0, 300)
						: "",
				});
				show(note);
			} catch {
				toast.error(t("notes.createError"));
			} finally {
				creatingRef.current = false;
				setCreating(false);
			}
		},
		[controller.store, onOpenChange, open, projectFilter, projects, show, t],
	);
	const createRef = useRef(create);
	createRef.current = create;
	useEffect(() => {
		const capture = (event: Event) => {
			const detail = (
				event as CustomEvent<{ sessionId: string | null; content: string }>
			).detail;
			if (detail.sessionId === scopeRef.current?.sessionId)
				void createRef.current(detail.content);
		};
		const keyboard = (event: KeyboardEvent) => {
			if (
				(event.metaKey || event.ctrlKey) &&
				event.shiftKey &&
				event.key.toLowerCase() === "n" &&
				!event.repeat
			) {
				event.preventDefault();
				void createRef.current(window.getSelection()?.toString() ?? "");
			}
		};
		window.addEventListener("dcc:capture-note", capture);
		window.addEventListener("keydown", keyboard);
		return () => {
			window.removeEventListener("dcc:capture-note", capture);
			window.removeEventListener("keydown", keyboard);
		};
	}, []);
	const filtered = controller.notes.filter(
		(note) =>
			note.status === statusFilter &&
			(projectFilter === "all" || note.projectId === projectFilter) &&
			(conversationFilter === "all" ||
				note.sourceTaskTitle === conversationFilter) &&
			`${note.title} ${note.content} ${note.contextSnapshot} ${note.sourceTaskTitle}`
				.toLocaleLowerCase()
				.includes(search.toLocaleLowerCase()),
	);
	const conversations = [
		...new Set(
			controller.notes
				.filter(
					(note) => projectFilter === "all" || note.projectId === projectFilter,
				)
				.map((note) => note.sourceTaskTitle)
				.filter(Boolean),
		),
	];
	const completionNotes = controller.notes.filter(
		(note) =>
			note.status === "open" &&
			note.implementationTaskId &&
			completionTaskIds.includes(note.implementationTaskId),
	);
	const positionHandlers = useRef(
		new Map<string, (value: FloatingState) => void>(),
	);
	useEffect(() => {
		const alive = new Set(controller.notes.map((note) => note.id));
		for (const id of positionHandlers.current.keys()) {
			if (!alive.has(id)) positionHandlers.current.delete(id);
		}
	}, [controller.notes]);
	const positionHandler = (id: string) => {
		if (!positionHandlers.current.has(id))
			positionHandlers.current.set(id, (value) => {
				setFloating((current) => {
					const prev = current[id];
					// An exiting balloon can still receive resize events until its
					// animation unmounts it. Moving it must not reopen a closed note.
					if (!prev) return current;
					if (
						prev.x === value.x &&
						prev.y === value.y &&
						prev.minimized === value.minimized
					)
						return current;
					return { ...current, [id]: value };
				});
			});
		return positionHandlers.current.get(id)!;
	};
	useEffect(() => {
		if (controller.loading || controller.error) return;
		positionsRef.current = { ...positionsRef.current, ...floating };
		const persist = () => {
			try {
				const alive = new Set(controller.notes.map((note) => note.id));
				positionsRef.current = Object.fromEntries(
					Object.entries(positionsRef.current).filter(([id]) => alive.has(id)),
				);
				localStorage.setItem(
					NOTE_POSITION_KEY,
					JSON.stringify(positionsRef.current),
				);
			} catch {
				/* Position persistence is optional. */
			}
		};
		const timer = setTimeout(persist, 250);
		window.addEventListener("pagehide", persist);
		return () => {
			clearTimeout(timer);
			window.removeEventListener("pagehide", persist);
		};
	}, [floating, controller.notes, controller.loading, controller.error]);
	const newTask = async (note: ProjectNote) => {
		if (note.implementationTaskId) {
			toast.info(t("notes.alreadyLinked"));
			return;
		}
		const taskId = await onCreateTask(note);
		controller.store.edit(note.id, { implementationTaskId: taskId });
		await controller.store.flush(note.id);
		toast.success(t("notes.taskDraftReady"));
	};
	return (
		<>
			<AnimatePresence>
				{!open &&
					controller.notes
						.filter((note) => floating[note.id])
						.map((note) => (
							<FloatingNote
								key={note.id}
								note={note}
								autoFocus={focusNoteId === note.id}
								position={floating[note.id]!}
								onPosition={positionHandler(note.id)}
								onClose={() => close(note.id)}
								controller={controller}
								onUse={() => {
									if (
										!scopeRef.current?.workspaceId ||
										scopeRef.current.projectId !== note.projectId
									) {
										toast.info(t("notes.useRequiresProjectTask", {
											project: note.projectName,
										}));
										return;
									}
									if (
										scopeRef.current.projectId !== scope?.projectId ||
										scopeRef.current?.workspaceId !== scope?.workspaceId ||
										scopeRef.current?.sessionId !== scope?.sessionId
									) {
										toast.info(t("notes.destinationChanged"));
										return;
									}
									onUse(notePrompt(note));
									close(note.id);
								}}
								onTask={() => newTask(note)}
								onDelete={() => setDeleteIds([note.id])}
								useDisabledReason={
									scope?.workspaceId && scope.projectId === note.projectId
										? null
										: t("notes.useRequiresProjectTask", {
												project: note.projectName,
											})
								}
							/>
						))}
			</AnimatePresence>
			<Dialog open={open} onOpenChange={onOpenChange}>
				<DialogContent
					className="notes-library sm:max-w-[920px]"
					onCloseAutoFocus={(event) => {
						if (focusNoteId && floating[focusNoteId]) {
							event.preventDefault();
							requestAnimationFrame(() =>
								document
									.querySelector<HTMLInputElement>(
										`[data-note-id="${CSS.escape(focusNoteId)}"] .note-title-input`,
									)
									?.focus(),
							);
						}
					}}
				>
					<DialogHeader>
						<div className="notes-eyebrow">
							<StickyNote size={14} />
							{t("notes.eyebrow")}
						</div>
						<DialogTitle className="notes-library-title">
							{t("notes.heading")}
						</DialogTitle>
						<DialogDescription>{t("notes.description")}</DialogDescription>
					</DialogHeader>
					<div className="notes-library-toolbar">
						<div className="notes-search">
							<Search size={15} />
							<input
								aria-label={t("notes.search")}
								placeholder={t("notes.search")}
								value={search}
								onChange={(event) => setSearch(event.target.value)}
							/>
						</div>
						<Button
							disabled={creating || controller.loading}
							onClick={() => {
								void create();
							}}
						>
							<Plus size={14} />
							{t("notes.new")}
						</Button>
					</div>
					<div className="notes-filters">
						<div className="notes-status-tabs">
							{(["open", "completed"] as const).map((status) => (
								<button
									type="button"
									key={status}
									aria-pressed={statusFilter === status}
									onClick={() => setStatusFilter(status)}
								>
									{status === "open" ? (
										<Circle size={12} />
									) : (
										<CheckCheck size={14} />
									)}
									{t(`notes.${status}`)}
									<span>
										{
											controller.notes.filter(
												(note) =>
													note.status === status &&
													(projectFilter === "all" ||
														note.projectId === projectFilter),
											).length
										}
									</span>
								</button>
							))}
						</div>
						<select
							aria-label={t("notes.projectFilter")}
							value={projectFilter}
							onChange={(event) => {
								setProjectFilter(event.target.value);
								setConversationFilter("all");
							}}
						>
							<option value="all">{t("notes.allProjects")}</option>
							{allProjects.map((project) => (
								<option key={project.id} value={project.id}>
									{project.name}
								</option>
							))}
						</select>
						<select
							aria-label={t("notes.conversationFilter")}
							value={conversationFilter}
							onChange={(event) => setConversationFilter(event.target.value)}
						>
							<option value="all">{t("notes.allConversations")}</option>
							{conversations.map((title) => (
								<option key={title} value={title}>
									{title}
								</option>
							))}
						</select>
					</div>
					<div className="notes-library-scroll">
						{controller.loading ? (
							<div className="notes-empty">
								<Loader2 className="animate-spin" />
								<p>{t("notes.loading")}</p>
							</div>
						) : controller.error ? (
							<div className="notes-empty">
								<p>{t("notes.loadError")}</p>
								<Button
									onClick={() => {
										void controller.store.load();
									}}
								>
									{t("notes.retry")}
								</Button>
							</div>
						) : filtered.length === 0 ? (
							<div className="notes-empty">
								<div className="notes-empty-orbit">
									<span />
									<span />
									<Lightbulb size={28} />
								</div>
								<h3>
									{t(
										statusFilter === "completed"
											? "notes.emptyCompleted"
											: search
												? "notes.noResults"
												: "notes.emptyTitle",
									)}
								</h3>
								<p>{t("notes.emptyHint")}</p>
							</div>
						) : (
							<div className="notes-grid">
								{filtered.map((note, index) => (
									<motion.button
										type="button"
										key={note.id}
										data-color={note.color}
										className="note-preview"
										onClick={() => show(note)}
										initial={reduced ? false : { opacity: 0, y: 8 }}
										animate={{ opacity: 1, y: 0 }}
										transition={{
											duration: 0.18,
											delay: Math.min(index * 0.025, 0.15),
										}}
									>
										<div className="note-preview-meta">
											<span>{note.projectName}</span>
											{note.status === "completed" ? (
												<CheckCheck size={14} />
											) : note.pinned ? (
												<Pin size={13} />
											) : (
												<ArrowUpRight size={14} />
											)}
										</div>
										<h3>{note.title || t("notes.untitled")}</h3>
										<p>{note.content || t("notes.contentPlaceholder")}</p>
										<div className="note-preview-origin">
											<MessageCircle size={12} />
											<span>
												{note.sourceTaskTitle || t("notes.projectNote")}
											</span>
										</div>
										{note.contextSnapshot && (
											<span className="note-context-tag">
												{t("notes.contextIncluded")}
											</span>
										)}
										{note.implementationTaskId && (
											<span className="note-context-tag">
												{t("notes.taskLinked")}
											</span>
										)}
									</motion.button>
								))}
							</div>
						)}
					</div>
					<footer className="notes-library-footer">
						<span>
							<CheckCheck size={13} />
							{t("notes.survivesCleanup")}
						</span>
						{statusFilter === "completed" && filtered.length > 0 && (
							<button
								onClick={() => setDeleteIds(filtered.map((note) => note.id))}
							>
								<Trash2 size={13} />
								{t("notes.clearCompleted")}
							</button>
						)}
					</footer>
				</DialogContent>
			</Dialog>
			<Dialog
				open={deleteIds.length > 0}
				onOpenChange={(value) => {
					if (!value && !busy) setDeleteIds([]);
				}}
			>
				<DialogContent>
					<DialogHeader>
						<DialogTitle>
							{t("notes.deleteTitle", { count: deleteIds.length })}
						</DialogTitle>
						<DialogDescription>
							{t("notes.deleteDescription")}
						</DialogDescription>
					</DialogHeader>
					<div className="flex justify-end gap-2">
						<Button
							variant="ghost"
							disabled={busy}
							onClick={() => setDeleteIds([])}
						>
							{t("notes.cancel")}
						</Button>
						<Button
							variant="destructive"
							disabled={busy}
							onClick={async () => {
								setBusy(true);
								try {
									await controller.store.remove(deleteIds);
									deleteIds.forEach(close);
									setDeleteIds([]);
								} catch {
									toast.error(t("notes.deleteError"));
								} finally {
									setBusy(false);
								}
							}}
						>
							{t("notes.delete")}
						</Button>
					</div>
				</DialogContent>
			</Dialog>
			<Dialog
				open={completionNotes.length > 0}
				onOpenChange={(value) => {
					if (!value && !busy) onCompletionHandled();
				}}
			>
				<DialogContent>
					<DialogHeader>
						<DialogTitle>{t("notes.completeLinkedTitle")}</DialogTitle>
						<DialogDescription>
							{t("notes.completeLinkedDescription")}
						</DialogDescription>
					</DialogHeader>
					<ul className="space-y-2">
						{completionNotes.map((note) => (
							<li key={note.id} className="rounded-lg bg-muted/40 px-3 py-2">
								{note.title || t("notes.untitled")}
							</li>
						))}
					</ul>
					<div className="flex justify-end gap-2">
						<Button
							variant="ghost"
							disabled={busy}
							onClick={onCompletionHandled}
						>
							{t("notes.keepOpen")}
						</Button>
						<Button
							disabled={busy}
							onClick={async () => {
								setBusy(true);
								try {
									for (const note of completionNotes) {
										controller.store.edit(note.id, { status: "completed" });
										await controller.store.flush(note.id);
										close(note.id);
									}
									onCompletionHandled();
								} catch {
									toast.error(t("notes.saveError"));
								} finally {
									setBusy(false);
								}
							}}
						>
							<Check size={14} />
							{t("notes.complete")}
						</Button>
					</div>
				</DialogContent>
			</Dialog>
		</>
	);
}
