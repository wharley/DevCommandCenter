import { motion, useReducedMotion } from "motion/react";
import {
	ArrowDownLeft,
	ArrowUpRight,
	Check,
	CheckCheck,
	ChevronDown,
	FolderOpen,
	Grip,
	Loader2,
	MessageCircle,
	Minus,
	Pin,
	Plus,
	RotateCcw,
	Trash2,
	X,
} from "lucide-react";
import {
	useCallback,
	useEffect,
	useId,
	useRef,
	useState,
	type CSSProperties,
	type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import type { NoteColor, ProjectNote } from "./notes-api";
import type { useProjectNotes } from "./use-project-notes";
export type FloatingState = { x: number; y: number; minimized: boolean };
const COLORS: NoteColor[] = ["amber", "mint", "violet", "sky"];

export function clampNotePosition(
	x: number,
	y: number,
	width: number,
	height: number,
	viewportWidth = window.innerWidth,
	viewportHeight = window.innerHeight,
) {
	return {
		x: Math.max(8, Math.min(x, viewportWidth - width - 8)),
		y: Math.max(40, Math.min(y, viewportHeight - height - 12)),
	};
}

function IconAction({
	label,
	children,
	onClick,
	active = false,
	disabled = false,
	describedBy,
}: {
	label: string;
	children: ReactNode;
	onClick: () => void;
	active?: boolean;
	disabled?: boolean;
	describedBy?: string;
}) {
	return (
		<button
			type="button"
			title={label}
			aria-label={label}
			aria-describedby={describedBy}
			aria-pressed={active || undefined}
			disabled={disabled}
			onClick={onClick}
			className={cn("note-icon", active && "note-icon-active")}
		>
			{children}
		</button>
	);
}

export function FloatingNote({
	note,
	position,
	onPosition,
	onClose,
	autoFocus = false,
	controller,
	onUse,
	onTask,
	onDelete,
	useDisabledReason,
}: {
	note: ProjectNote;
	position: FloatingState;
	onPosition: (value: FloatingState) => void;
	onClose: () => void;
	autoFocus?: boolean;
	controller: ReturnType<typeof useProjectNotes>;
	onUse: () => void;
	onTask: () => Promise<void>;
	onDelete: () => void;
	useDisabledReason: string | null;
}) {
	const { t } = useTranslation("common");
	const useDisabledReasonId = useId();
	const reduced = useReducedMotion();
	const ref = useRef<HTMLDivElement>(null);
	const drag = useRef<{
		x: number;
		y: number;
		left: number;
		top: number;
	} | null>(null);
	const [dragging, setDragging] = useState(false);
	const [busy, setBusy] = useState(false);
	const [contextOpen, setContextOpen] = useState(false);
	const positionRef = useRef(position);
	positionRef.current = position;
	const saving = controller.saving.includes(note.id);
	const failed = controller.failed.includes(note.id);
	const mutate = controller.store.edit.bind(controller.store, note.id);
	const place = useCallback(
		(x: number, y: number) => {
			onPosition({
				...positionRef.current,
				...clampNotePosition(
					x,
					y,
					ref.current?.offsetWidth ?? 340,
					ref.current?.offsetHeight ?? 450,
				),
			});
		},
		[onPosition],
	);
	useEffect(() => {
		const resize = () => place(positionRef.current.x, positionRef.current.y);
		resize();
		window.addEventListener("resize", resize);
		return () => window.removeEventListener("resize", resize);
	}, [place, position.minimized, contextOpen]);
	const perform = async (action: () => Promise<void>) => {
		setBusy(true);
		try {
			await controller.store.flush(note.id);
			await action();
		} catch {
			toast.error(t("notes.saveError"));
		} finally {
			setBusy(false);
		}
	};
	return (
		<motion.div
			layout={reduced ? false : "size"}
			ref={ref}
			data-dcc-browser-occluder="true"
			data-note-id={note.id}
			className={cn(
				"note-balloon",
				position.minimized && "note-balloon-mini",
				dragging && "note-dragging",
			)}
			data-color={note.color}
			role="region"
			aria-label={note.title || t("notes.untitled")}
			style={
				{
					left: position.x,
					top: position.y,
					zIndex: dragging ? 49 : 45,
				} as CSSProperties
			}
			initial={reduced ? false : { opacity: 0, scale: 0.7, y: 30, rotate: -5 }}
			animate={{
				opacity: 1,
				scale: 1,
				y: 0,
				rotate: dragging && !reduced ? 1.5 : 0,
			}}
			exit={
				reduced ? { opacity: 0 } : { opacity: 0, scale: 0.6, y: -25, rotate: 5 }
			}
			transition={{ type: "spring", stiffness: 390, damping: 28 }}
		>
			<header className="note-balloon-header">
				<button
					type="button"
					className="note-drag-handle"
					aria-label={t("notes.move")}
					title={t("notes.move")}
					onPointerDown={(event) => {
						if (event.button !== 0) return;
						event.currentTarget.setPointerCapture(event.pointerId);
						drag.current = {
							x: event.clientX,
							y: event.clientY,
							left: position.x,
							top: position.y,
						};
						setDragging(true);
					}}
					onPointerMove={(event) => {
						if (drag.current)
							place(
								drag.current.left + event.clientX - drag.current.x,
								drag.current.top + event.clientY - drag.current.y,
							);
					}}
					onPointerUp={() => {
						drag.current = null;
						setDragging(false);
					}}
					onPointerCancel={() => {
						drag.current = null;
						setDragging(false);
					}}
					onKeyDown={(event) => {
						const delta = event.shiftKey ? 40 : 12;
						const offsets: Record<string, number[]> = {
							ArrowLeft: [-delta, 0],
							ArrowRight: [delta, 0],
							ArrowUp: [0, -delta],
							ArrowDown: [0, delta],
						};
						const offset = offsets[event.key];
						if (offset) {
							event.preventDefault();
							place(position.x + offset[0]!, position.y + offset[1]!);
						}
					}}
				>
					<Grip size={15} />
					<span>
						{position.minimized
							? note.title || t("notes.untitled")
							: t(
									note.status === "completed"
										? "notes.realizedIdea"
										: "notes.idea",
								)}
					</span>
				</button>
				<IconAction
					label={note.pinned ? t("notes.unpin") : t("notes.pin")}
					active={note.pinned}
					onClick={() => mutate({ pinned: !note.pinned })}
				>
					<Pin size={13} />
				</IconAction>
				<IconAction
					label={position.minimized ? t("notes.expand") : t("notes.minimize")}
					onClick={() =>
						onPosition({ ...position, minimized: !position.minimized })
					}
				>
					{position.minimized ? (
						<ArrowUpRight size={14} />
					) : (
						<Minus size={14} />
					)}
				</IconAction>
				<IconAction
					label={t("notes.close")}
					onClick={() => {
						void perform(async () => onClose());
					}}
					disabled={busy}
				>
					<X size={14} />
				</IconAction>
			</header>
			{!position.minimized && (
				<div className="note-balloon-body">
					<div className="note-project">
						<FolderOpen size={12} />
						<span>{note.projectName}</span>
						<div className="note-colors">
							{COLORS.map((color) => (
								<button
									key={color}
									data-color={color}
									className="note-swatch"
									aria-label={t(`notes.colors.${color}`)}
									aria-pressed={note.color === color}
									onClick={() => mutate({ color })}
								>
									{note.color === color && <Check size={10} />}
								</button>
							))}
						</div>
					</div>
					<input
						className="note-title-input"
						autoFocus={autoFocus}
						aria-label={t("notes.titleLabel")}
						placeholder={t("notes.titlePlaceholder")}
						value={note.title}
						maxLength={160}
						onChange={(event) => mutate({ title: event.target.value })}
					/>
					<textarea
						className="note-content-input"
						aria-label={t("notes.contentLabel")}
						placeholder={t("notes.contentPlaceholder")}
						value={note.content}
						maxLength={20_000}
						onChange={(event) => mutate({ content: event.target.value })}
						onBlur={() => {
							void controller.store.flush(note.id).catch(() => {});
						}}
					/>
					{note.sourceTaskTitle && (
						<div className="note-source">
							<MessageCircle size={12} />
							<span>{note.sourceTaskTitle}</span>
							{!note.sourceWorkspaceId && (
								<span className="note-source-deleted">
									{t("notes.sourceDeleted")}
								</span>
							)}
						</div>
					)}
					{note.contextSnapshot && (
						<div className="note-context">
							<button
								type="button"
								onClick={() => setContextOpen(!contextOpen)}
								aria-expanded={contextOpen}
							>
								<span>{t("notes.savedContext")}</span>
								<ChevronDown
									size={13}
									className={contextOpen ? "rotate-180" : ""}
								/>
							</button>
							{contextOpen && <pre>{note.contextSnapshot}</pre>}
						</div>
					)}
					<div className="note-save-status" role="status">
						{failed ? (
							<button
								onClick={() => {
									void controller.store
										.retry(note.id)
										.catch(() => toast.error(t("notes.saveError")));
								}}
							>
								{t("notes.retrySave")}
							</button>
						) : saving ? (
							<>
								<Loader2 size={11} className="animate-spin" />
								{t("notes.saving")}
							</>
						) : (
							<>
								<CheckCheck size={12} />
								{t("notes.saved")}
							</>
						)}
					</div>
					{useDisabledReason && (
						<p id={useDisabledReasonId} className="note-use-hint">
							{useDisabledReason}
						</p>
					)}
					<div className="note-balloon-actions">
						<button
							type="button"
							className="note-primary-action"
							disabled={busy || (!note.content.trim() && !note.title.trim())}
							onClick={() => {
								void perform(onTask);
							}}
						>
							<Plus size={14} />
							{t("notes.createTask")}
						</button>
						<IconAction
							label={t("notes.useHere")}
							disabled={Boolean(useDisabledReason) || busy}
							describedBy={useDisabledReason ? useDisabledReasonId : undefined}
							onClick={() => {
								void perform(async () => onUse());
							}}
						>
							<ArrowDownLeft size={16} />
						</IconAction>
						<IconAction
							label={
								note.status === "completed"
									? t("notes.reopen")
									: t("notes.complete")
							}
							disabled={busy}
							onClick={() => {
								void perform(async () => {
									mutate({
										status: note.status === "completed" ? "open" : "completed",
									});
									await controller.store.flush(note.id);
									onClose();
								});
							}}
						>
							{note.status === "completed" ? (
								<RotateCcw size={15} />
							) : (
								<Check size={16} />
							)}
						</IconAction>
						<IconAction
							label={t("notes.delete")}
							onClick={onDelete}
							disabled={busy}
						>
							<Trash2 size={14} />
						</IconAction>
					</div>
				</div>
			)}
		</motion.div>
	);
}
