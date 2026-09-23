import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Maximize2, Minimize2, Monitor, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
	getComputerUsePreviewFrame,
	getComputerUseStatus,
	isComputerUseDesktop,
	type ComputerUseActivityEvent,
	type ComputerUsePreviewFrame,
} from "@/lib/computer-use-api";

type Activity = Pick<ComputerUseActivityEvent, "activityId" | "tool" | "phase">;

function activityLabelKey(tool: string): string {
	switch (tool) {
		case "dcc_computer_status": return "status";
		case "dcc_computer_request_control": return "requesting";
		case "dcc_computer_capture": return "observing";
		case "dcc_computer_click": return "clicking";
		case "dcc_computer_scroll": return "scrolling";
		case "dcc_computer_type": return "typing";
		case "dcc_computer_key": return "key";
		default: return "working";
	}
}

export function ComputerUsePreview({
	sessionId,
	isTurnActive,
	onStop,
}: {
	sessionId: string | null;
	isTurnActive: boolean;
	onStop: () => void;
}) {
	const { t } = useTranslation("common");
	const [frame, setFrame] = useState<ComputerUsePreviewFrame | null>(null);
	const [activity, setActivity] = useState<Activity | null>(null);
	const [hidden, setHidden] = useState(false);
	const [expanded, setExpanded] = useState(false);

	useEffect(() => {
		setFrame(null);
		setActivity(null);
		setHidden(false);
		setExpanded(false);
		if (!sessionId || !isComputerUseDesktop()) return;

		let disposed = false;
		let clearActivityTimer: number | undefined;
		let grantExpiryTimer: number | undefined;
		let unlistenActivity: (() => void) | undefined;
		let unlistenPreview: (() => void) | undefined;
		let unlistenCleared: (() => void) | undefined;
		const refreshFrame = () => {
			void getComputerUsePreviewFrame(sessionId)
				.then((next) => {
					if (!disposed) setFrame(next);
				})
				.catch(() => undefined);
		};
		const refreshGrantExpiry = () => {
			if (grantExpiryTimer !== undefined) window.clearTimeout(grantExpiryTimer);
			void getComputerUseStatus(sessionId, false).then((status) => {
				if (disposed) return;
				if (!status.grant.armed) {
					setFrame(null);
					return;
				}
				grantExpiryTimer = window.setTimeout(() => {
					void getComputerUsePreviewFrame(sessionId).then((latest) => {
						if (!disposed && latest === null) setFrame(null);
					}).catch(() => undefined);
				}, Math.max(0, status.grant.remainingMs));
			}).catch(() => undefined);
		};

		void listen<ComputerUseActivityEvent>("computer-use-activity", ({ payload }) => {
			if (payload.sessionId !== sessionId) return;
			if (clearActivityTimer !== undefined) window.clearTimeout(clearActivityTimer);
			setActivity({
				activityId: payload.activityId,
				tool: payload.tool,
				phase: payload.phase,
			});
			if (payload.phase !== "started") {
				clearActivityTimer = window.setTimeout(() => {
					setActivity((current) => current?.activityId === payload.activityId ? null : current);
				}, 4_000);
			}
		}).then((unsubscribe) => {
			if (disposed) unsubscribe();
			else unlistenActivity = unsubscribe;
		});
		void listen<string>("computer-use-preview-updated", ({ payload }) => {
			if (payload === sessionId) {
				refreshFrame();
				refreshGrantExpiry();
			}
		}).then((unsubscribe) => {
			if (disposed) unsubscribe();
			else unlistenPreview = unsubscribe;
		});
		void listen<string>("computer-use-preview-cleared", ({ payload }) => {
			if (payload !== sessionId) return;
			setFrame(null);
			setActivity(null);
		}).then((unsubscribe) => {
			if (disposed) unsubscribe();
			else unlistenCleared = unsubscribe;
		});
		refreshGrantExpiry();
		refreshFrame();

		return () => {
			disposed = true;
			if (clearActivityTimer !== undefined) window.clearTimeout(clearActivityTimer);
			if (grantExpiryTimer !== undefined) window.clearTimeout(grantExpiryTimer);
			unlistenActivity?.();
			unlistenPreview?.();
			unlistenCleared?.();
		};
	}, [sessionId]);

	if (!sessionId || (!frame && !activity)) return null;
	if (hidden) {
		return frame ? (
			<button
				type="button"
				className="pointer-events-auto absolute bottom-4 right-4 z-40 flex items-center gap-2 rounded-full border border-border bg-background/95 px-3 py-2 text-xs font-medium text-foreground shadow-lg backdrop-blur"
				onClick={() => setHidden(false)}
			>
				<Monitor className="size-3.5 text-primary" aria-hidden />
				{t("settings.computerUse.preview.reopen")}
			</button>
		) : null;
	}

	const toolLabel = activity
		? t(`settings.computerUse.preview.tools.${activityLabelKey(activity.tool)}`)
		: t("settings.computerUse.preview.ready");
	const activityState = activity?.phase === "started"
		? t("settings.computerUse.preview.working")
		: activity?.phase === "failed"
			? t("settings.computerUse.preview.failed")
			: activity
				? t("settings.computerUse.preview.done")
				: isTurnActive
					? t("settings.computerUse.preview.working")
					: t("settings.computerUse.preview.ready");

	return (
		<aside
			aria-label={t("settings.computerUse.preview.title")}
			className="pointer-events-none absolute bottom-4 right-4 z-40 w-[min(320px,calc(100%-2rem))] overflow-hidden rounded-xl border border-border/80 bg-background/95 text-foreground shadow-2xl backdrop-blur-xl"
			data-computer-use-preview={sessionId}
		>
			<div className="pointer-events-auto flex items-center gap-2 border-b border-border/70 px-3 py-2">
				<span className="relative flex size-2 shrink-0">
					{activity?.phase === "started" ? <span className="absolute inline-flex size-full animate-ping rounded-full bg-primary opacity-60" /> : null}
					<span className={`relative inline-flex size-2 rounded-full ${activity?.phase === "failed" ? "bg-destructive" : activity?.phase === "started" ? "bg-primary" : "bg-emerald-500"}`} />
				</span>
				<div className="min-w-0 flex-1">
					<div className="truncate text-xs font-medium">{frame?.target.name ?? t("settings.computerUse.preview.title")}</div>
					<div className="truncate text-[10px] text-muted-foreground">{toolLabel} · {activityState}</div>
				</div>
				{isTurnActive || activity?.phase === "started" ? (
					<button type="button" className="rounded px-2 py-1 text-[10px] font-medium text-destructive hover:bg-destructive/10" onClick={onStop}>
						{t("settings.computerUse.preview.stop")}
					</button>
				) : null}
				<button type="button" className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground" aria-label={t(expanded ? "settings.computerUse.preview.shrink" : "settings.computerUse.preview.expand")} onClick={() => setExpanded((current) => !current)}>
					{expanded ? <Minimize2 className="size-3.5" aria-hidden /> : <Maximize2 className="size-3.5" aria-hidden />}
				</button>
				<button type="button" className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground" aria-label={t("settings.computerUse.preview.hide")} onClick={() => setHidden(true)}>
					<X className="size-3.5" aria-hidden />
				</button>
			</div>
			{frame ? (
				<div className={`pointer-events-none bg-black ${expanded ? "max-h-[min(60vh,520px)]" : "max-h-52"}`}>
					<img
						className="mx-auto block max-h-[min(60vh,520px)] w-full object-contain"
						src={`data:${frame.mimeType};base64,${frame.imageBase64}`}
						alt={t("settings.computerUse.preview.frameAlt", { app: frame.target.name })}
					/>
				</div>
			) : (
				<div className="flex h-24 items-center justify-center gap-2 bg-muted/25 text-xs text-muted-foreground" aria-live="polite">
					<Monitor className="size-4" aria-hidden />
					{t("settings.computerUse.preview.waitingFrame")}
				</div>
			)}
		</aside>
	);
}
