import { useCallback, useEffect, useRef, useState } from "react";
import { Activity, ChevronDown, Globe2, History, LoaderCircle, RefreshCw, Shield, ShieldCheck, Sparkles, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Popover, PopoverAnchor, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import {
	hideBrowser,
	getBrowserControlStatus,
	armBrowserControl,
	disarmBrowserControl,
	readBrowserAudit,
	listenBrowserState,
	navigateBrowser,
	openBrowser,
	reloadBrowser,
	setBrowserBounds,
	extractBrowserContext,
	anchorFromBrowserContext,
	startBrowserEvidenceCapture,
	readBrowserEvidenceCapture,
	type BrowserAgentContext,
	type BrowserAuditRecord,
	type BrowserBounds,
	type BrowserSnapshot,
} from "./browser-api";
import { isBrowserOccluded, useBrowserOcclusion } from "./browser-occlusion";
import type { BrowserEvidenceCapture } from "./browser-agent-context";
import { resolveHumanBrowserAddress } from "./browser-address";

type WorkspaceBrowserSurfaceProps = {
	workspaceId: string;
	sessionId: string | null;
	onClose: () => void;
	onSendToAgent?: (context: BrowserAgentContext) => void;
	/** Receives a drained console/resource capture started by an explicit gesture. */
	onSendEvidenceToAgent?: (capture: BrowserEvidenceCapture) => void;
	/** Splitter drags hide the native view for the duration of the resize. */
	forceOccluded?: boolean;
};

/**
 * Expand a logical DOM rectangle to complete device pixels before handing it
 * to the native child WebView. Native frames are integer-sized on some
 * platforms, so independently rounding a fractional height can expose the
 * renderer's background along the bottom edge.
 */
export function snapBrowserBoundsToDevicePixels(
	bounds: BrowserBounds,
	devicePixelRatio = 1,
): BrowserBounds {
	const scale = Number.isFinite(devicePixelRatio) && devicePixelRatio > 0
		? devicePixelRatio
		: 1;
	const x = Math.max(0, bounds.x);
	const y = Math.max(0, bounds.y);
	const width = Math.max(1, bounds.width);
	const height = Math.max(1, bounds.height);
	const left = Math.floor(x * scale);
	const top = Math.floor(y * scale);
	const right = Math.ceil((x + width) * scale);
	const bottom = Math.ceil((y + height) * scale);
	return {
		x: left / scale,
		y: top / scale,
		width: Math.max(1, (right - left) / scale),
		height: Math.max(1, (bottom - top) / scale),
	};
}

export function readBrowserBounds(element: HTMLElement): BrowserBounds {
	const rect = element.getBoundingClientRect();
	return snapBrowserBoundsToDevicePixels(
		{
			x: rect.left,
			y: rect.top,
			width: rect.width,
			height: rect.height,
		},
		typeof window !== "undefined" ? window.devicePixelRatio : 1,
	);
}

/** A single expiration timer is sufficient; the backend remains authoritative. */
export function browserControlExpiryDelay(remainingMs: number): number {
	return Math.max(0, Math.ceil(Number.isFinite(remainingMs) ? remainingMs : 0));
}

type BrowserAuditRequestScope = {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
};

/** Keeps late audit responses from a closed, changed, or reopened Browser scope out of the viewer. */
export function isCurrentBrowserAuditRequest(input: {
	requestId: number;
	currentRequestId: number;
	open: boolean;
	expected: BrowserAuditRequestScope;
	current: BrowserAuditRequestScope | null;
}): boolean {
	return input.open
		&& input.requestId === input.currentRequestId
		&& input.current?.workspaceId === input.expected.workspaceId
		&& input.current.sessionId === input.expected.sessionId
		&& input.current.lifecycleToken === input.expected.lifecycleToken;
}

export function browserAuditTime(timestampMs: number): Date | null {
	if (!Number.isSafeInteger(timestampMs) || timestampMs < 0) return null;
	const date = new Date(timestampMs);
	return Number.isNaN(date.getTime()) ? null : date;
}

export function newestFirstBrowserAuditRecords(records: BrowserAuditRecord[]): BrowserAuditRecord[] {
	return [...records].sort((left, right) => {
		const leftTime = browserAuditTime(left.timestampMs)?.getTime() ?? -1;
		const rightTime = browserAuditTime(right.timestampMs)?.getTime() ?? -1;
		return rightTime - leftTime;
	});
}

export function WorkspaceBrowserSurface({
	workspaceId,
	sessionId,
	onClose,
	onSendToAgent,
	onSendEvidenceToAgent,
	forceOccluded = false,
}: WorkspaceBrowserSurfaceProps) {
	const viewportRef = useRef<HTMLDivElement | null>(null);
	const boundsFrameRef = useRef<number | null>(null);
	const { t } = useTranslation("common");
	const [snapshot, setSnapshot] = useState<BrowserSnapshot | null>(null);
	const [address, setAddress] = useState("");
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);
	const [sendingContext, setSendingContext] = useState(false);
	const [controlStatus, setControlStatus] = useState<{ armed: boolean; remainingMs: number }>({ armed: false, remainingMs: 0 });
	const [controlBusy, setControlBusy] = useState(false);
	const [controlSecondsLeft, setControlSecondsLeft] = useState(0);
	// One human-started evidence capture at a time. The backend owns the page
	// token and expiry; this state only remembers the opaque handle so the
	// person can collect it, and it is dropped on any scope/lifecycle change.
	const [evidenceCapture, setEvidenceCapture] = useState<{
		captureId: string;
		lifecycleToken: number;
		url: string;
		title: string | null;
		startedAtMs: number;
		expiresAtMs: number;
	} | null>(null);
	const [evidenceBusy, setEvidenceBusy] = useState(false);
	const [evidenceSecondsLeft, setEvidenceSecondsLeft] = useState(0);
	const [auditOpen, setAuditOpen] = useState(false);
	const [auditRecords, setAuditRecords] = useState<BrowserAuditRecord[] | null>(null);
	const [auditLoading, setAuditLoading] = useState(false);
	const [auditFailed, setAuditFailed] = useState(false);
	const [lifecycleToken, setLifecycleToken] = useState<number | null>(null);
	const lifecycleTokenRef = useRef<number | null>(null);
	const controlExpiryTimerRef = useRef<number | null>(null);
	const auditRequestRef = useRef(0);
	const auditOpenRef = useRef(false);
	const auditScopeRef = useRef<BrowserAuditRequestScope | null>(null);
	const pendingAuditOpenRef = useRef(false);
	auditOpenRef.current = auditOpen;
	auditScopeRef.current = lifecycleToken === null ? null : { workspaceId, sessionId, lifecycleToken };
	const updateLifecycleToken = useCallback((next: number) => {
		lifecycleTokenRef.current = next;
		setLifecycleToken(next);
	}, []);
	const clearControlExpiryTimer = useCallback(() => {
		if (controlExpiryTimerRef.current !== null) {
			window.clearTimeout(controlExpiryTimerRef.current);
			controlExpiryTimerRef.current = null;
		}
	}, []);

	useBrowserOcclusion({
		viewportRef,
		workspaceId,
		sessionId,
		lifecycleToken,
		forceOccluded,
	});

	const updateBounds = useCallback(() => {
		const viewport = viewportRef.current;
		if (!viewport) return;
		const token = lifecycleTokenRef.current;
		if (token === null) return;
		void setBrowserBounds({ workspaceId, sessionId, lifecycleToken: token, bounds: readBrowserBounds(viewport) }).catch(() => {
			// The child may not exist yet during the first layout frame.
		});
	}, [sessionId, workspaceId]);

	const scheduleBoundsUpdate = useCallback(() => {
		if (boundsFrameRef.current !== null) return;
		boundsFrameRef.current = requestAnimationFrame(() => {
			boundsFrameRef.current = null;
			updateBounds();
		});
	}, [updateBounds]);

	useEffect(() => {
		let cancelled = false;
		const viewport = viewportRef.current;
		if (!viewport) return;
		const bounds = readBrowserBounds(viewport);
		setLoading(true);
		setError(null);
		// This surface only mounts after the user explicitly opens Browser from
		// the workbench, so durable URL restoration remains opt-in rather than a
		// side effect of app/workspace remounts.
		void openBrowser({ workspaceId, sessionId, bounds, restoreLastUrl: true, initialOccluded: isBrowserOccluded(viewport) })
			.then((next) => {
				if (cancelled) {
					void hideBrowser({
						workspaceId,
						sessionId,
						lifecycleToken: next.lifecycleToken,
					}).catch(() => {});
					return;
				}
				setSnapshot(next);
				updateLifecycleToken(next.lifecycleToken);
				setAddress(next.url ?? "");
				setLoading(false);
				scheduleBoundsUpdate();
			})
			.catch((reason: unknown) => {
				if (cancelled) return;
				setLoading(false);
				setError(reason instanceof Error ? reason.message : String(reason));
			});
		return () => {
			cancelled = true;
		};
	}, [scheduleBoundsUpdate, sessionId, workspaceId]);

	useEffect(() => {
		let unlisten: (() => void) | undefined;
		let disposed = false;
		void listenBrowserState((next) => {
			if (disposed) return;
			if (next.workspaceId !== workspaceId || next.sessionId !== sessionId) return;
			if (next.lifecycleToken < (lifecycleTokenRef.current ?? 0)) return;
			setSnapshot(next);
			updateLifecycleToken(next.lifecycleToken);
			if (next.url) setAddress(next.url);
			setLoading(false);
		})
			.then((dispose) => {
				if (disposed) {
					dispose();
				} else {
					unlisten = dispose;
				}
			})
			.catch(() => {});
		return () => {
			disposed = true;
			unlisten?.();
		};
	}, [sessionId, updateLifecycleToken, workspaceId]);

	useEffect(() => {
		clearControlExpiryTimer();
		setControlStatus({ armed: false, remainingMs: 0 });
		if (!sessionId || lifecycleToken === null) return;
		let cancelled = false;
		void getBrowserControlStatus({ workspaceId, sessionId, lifecycleToken })
			.then((next) => {
				if (!cancelled && lifecycleTokenRef.current === lifecycleToken) setControlStatus(next);
			})
			.catch((reason: unknown) => {
				if (!cancelled) setError(reason instanceof Error ? reason.message : String(reason));
			});
		return () => {
			cancelled = true;
			clearControlExpiryTimer();
		};
	}, [clearControlExpiryTimer, lifecycleToken, sessionId, workspaceId]);

	useEffect(() => {
		clearControlExpiryTimer();
		if (!controlStatus.armed) return;
		controlExpiryTimerRef.current = window.setTimeout(() => {
			controlExpiryTimerRef.current = null;
			setControlStatus({ armed: false, remainingMs: 0 });
		}, browserControlExpiryDelay(controlStatus.remainingMs));
		return clearControlExpiryTimer;
	}, [clearControlExpiryTimer, controlStatus]);

	useEffect(() => {
		// Display-only countdown for the pill; the backend and the expiry timer
		// above stay authoritative for when the grant actually ends.
		if (!controlStatus.armed) {
			setControlSecondsLeft(0);
			return;
		}
		const expiresAtMs = Date.now() + browserControlExpiryDelay(controlStatus.remainingMs);
		const tick = () => {
			setControlSecondsLeft(Math.max(0, Math.ceil((expiresAtMs - Date.now()) / 1000)));
		};
		tick();
		const interval = window.setInterval(tick, 1_000);
		return () => window.clearInterval(interval);
	}, [controlStatus]);

	useEffect(() => {
		// A Browser scope/lifecycle transition makes every in-flight audit read
		// stale. Closing also removes the marked portal before the next native
		// surface can become visible.
		auditRequestRef.current += 1;
		auditOpenRef.current = false;
		setAuditOpen(false);
		setAuditRecords(null);
		setAuditLoading(false);
		setAuditFailed(false);
	}, [lifecycleToken, sessionId, workspaceId]);

	useEffect(() => {
		return () => {
			clearControlExpiryTimer();
			const lifecycleToken = lifecycleTokenRef.current;
			if (lifecycleToken === null) return;
			void hideBrowser({ workspaceId, sessionId, lifecycleToken }).catch(() => {});
		};
	}, [clearControlExpiryTimer, sessionId, workspaceId]);

	useEffect(() => {
		const viewport = viewportRef.current;
		if (!viewport) return;
		const observer = typeof ResizeObserver === "undefined"
			? null
			: new ResizeObserver(scheduleBoundsUpdate);
		observer?.observe(viewport);
		window.addEventListener("resize", scheduleBoundsUpdate);
		return () => {
			observer?.disconnect();
			window.removeEventListener("resize", scheduleBoundsUpdate);
			if (boundsFrameRef.current !== null) {
				cancelAnimationFrame(boundsFrameRef.current);
				boundsFrameRef.current = null;
			}
		};
	}, [scheduleBoundsUpdate]);

	const handleNavigate = useCallback(() => {
		const lifecycleToken = lifecycleTokenRef.current;
		if (lifecycleToken === null) return;
		const url = resolveHumanBrowserAddress(address);
		if (!url) return;
		setAddress(url);
		setError(null);
		setLoading(true);
		void navigateBrowser({ workspaceId, sessionId, lifecycleToken, url })
			.then((next) => {
				setSnapshot(next);
				setAddress(next.url ?? url);
			})
			.catch((reason: unknown) => {
				setError(reason instanceof Error ? reason.message : String(reason));
			})
			.finally(() => setLoading(false));
	}, [address, sessionId, workspaceId]);

	const handleReload = useCallback(() => {
		const lifecycleToken = lifecycleTokenRef.current;
		if (lifecycleToken === null) return;
		setError(null);
		setLoading(true);
		void reloadBrowser({ workspaceId, sessionId, lifecycleToken })
			.catch((reason: unknown) => {
				setError(reason instanceof Error ? reason.message : String(reason));
			})
			.finally(() => setLoading(false));
	}, [sessionId, workspaceId]);

	const handleSendToAgent = useCallback(() => {
		const lifecycleToken = lifecycleTokenRef.current;
		if (lifecycleToken === null) return;
		if (!onSendToAgent || sendingContext) return;
		setError(null);
		setSendingContext(true);
		void extractBrowserContext({ workspaceId, sessionId, lifecycleToken })
			.then(onSendToAgent)
			.catch((reason: unknown) => {
				setError(reason instanceof Error ? reason.message : String(reason));
			})
			.finally(() => setSendingContext(false));
	}, [onSendToAgent, sendingContext, sessionId, workspaceId]);

	const handleControlToggle = useCallback(() => {
		const token = lifecycleTokenRef.current;
		if (!sessionId || token === null || controlBusy) return;
		setError(null);
		setControlBusy(true);
		const input = { workspaceId, sessionId, lifecycleToken: token };
		void (controlStatus.armed ? disarmBrowserControl(input) : armBrowserControl(input))
			.then((next) => {
				if (lifecycleTokenRef.current === token) setControlStatus(next);
			})
			.catch((reason: unknown) => {
				setError(reason instanceof Error ? reason.message : String(reason));
			})
			.finally(() => setControlBusy(false));
	}, [controlBusy, controlStatus.armed, sessionId, workspaceId]);

	useEffect(() => {
		// A capture is bound to the page, lifecycle and grant it was started on.
		setEvidenceCapture(null);
	}, [lifecycleToken, sessionId, workspaceId]);

	useEffect(() => {
		if (!evidenceCapture) {
			setEvidenceSecondsLeft(0);
			return;
		}
		if (!controlStatus.armed) {
			// Reading requires the grant that started the capture.
			setEvidenceCapture(null);
			setError(t("browser.evidence.expired"));
			return;
		}
		const tick = () => {
			const remainingMs = evidenceCapture.expiresAtMs - Date.now();
			if (remainingMs <= 0) {
				setEvidenceCapture(null);
				setError(t("browser.evidence.expired"));
				return;
			}
			setEvidenceSecondsLeft(Math.ceil(remainingMs / 1000));
		};
		tick();
		// Only runs while a capture is active; no idle polling.
		const interval = window.setInterval(tick, 1_000);
		return () => window.clearInterval(interval);
	}, [controlStatus.armed, evidenceCapture, t]);

	const handleStartEvidence = useCallback(() => {
		const lifecycleToken = lifecycleTokenRef.current;
		if (lifecycleToken === null || !sessionId) return;
		if (!onSendEvidenceToAgent || evidenceBusy || evidenceCapture || !controlStatus.armed) return;
		setError(null);
		setEvidenceBusy(true);
		void extractBrowserContext({ workspaceId, sessionId, lifecycleToken })
			.then(async (context) => {
				const handle = await startBrowserEvidenceCapture(
					anchorFromBrowserContext(context, lifecycleToken),
				);
				if (lifecycleTokenRef.current !== lifecycleToken) return;
				const now = Date.now();
				setEvidenceCapture({
					captureId: handle.captureId,
					lifecycleToken,
					url: context.url,
					title: context.title,
					startedAtMs: now,
					expiresAtMs: now + handle.remainingMs,
				});
			})
			.catch((reason: unknown) => {
				setError(reason instanceof Error ? reason.message : String(reason));
			})
			.finally(() => setEvidenceBusy(false));
	}, [controlStatus.armed, evidenceBusy, evidenceCapture, onSendEvidenceToAgent, sessionId, workspaceId]);

	const handleCollectEvidence = useCallback(() => {
		const capture = evidenceCapture;
		if (!capture || !onSendEvidenceToAgent || evidenceBusy) return;
		if (lifecycleTokenRef.current !== capture.lifecycleToken) {
			setEvidenceCapture(null);
			return;
		}
		setError(null);
		setEvidenceBusy(true);
		void readBrowserEvidenceCapture({ workspaceId, sessionId, captureId: capture.captureId })
			.then((result) => {
				onSendEvidenceToAgent({
					workspaceId,
					sessionId,
					url: capture.url,
					title: capture.title,
					startedAt: new Date(capture.startedAtMs).toISOString(),
					windowMs: Date.now() - capture.startedAtMs,
					result,
				});
			})
			.catch((reason: unknown) => {
				setError(reason instanceof Error ? reason.message : String(reason));
			})
			.finally(() => {
				// The handle is one-shot either way.
				setEvidenceCapture(null);
				setEvidenceBusy(false);
			});
	}, [evidenceBusy, evidenceCapture, onSendEvidenceToAgent, sessionId, workspaceId]);

	const handleDiscardEvidence = useCallback(() => {
		// The backend wrapper unwinds itself at expiry; nothing is read.
		setEvidenceCapture(null);
	}, []);

	const loadAudit = useCallback((token: number) => {
		const expected = { workspaceId, sessionId, lifecycleToken: token };
		const requestId = auditRequestRef.current + 1;
		auditRequestRef.current = requestId;
		setAuditLoading(true);
		setAuditFailed(false);
		void readBrowserAudit({ ...expected, limit: 50 })
			.then((records) => {
				if (!isCurrentBrowserAuditRequest({
					requestId,
					currentRequestId: auditRequestRef.current,
					open: auditOpenRef.current,
					expected,
					current: auditScopeRef.current,
				})) return;
				setAuditRecords(newestFirstBrowserAuditRecords(records));
			})
			.catch(() => {
				if (!isCurrentBrowserAuditRequest({
					requestId,
					currentRequestId: auditRequestRef.current,
					open: auditOpenRef.current,
					expected,
					current: auditScopeRef.current,
				})) return;
				setAuditFailed(true);
			})
			.finally(() => {
				if (!isCurrentBrowserAuditRequest({
					requestId,
					currentRequestId: auditRequestRef.current,
					open: auditOpenRef.current,
					expected,
					current: auditScopeRef.current,
				})) return;
				setAuditLoading(false);
			});
	}, [sessionId, workspaceId]);

	const handleAuditOpenChange = useCallback((nextOpen: boolean) => {
		if (!nextOpen) {
			auditRequestRef.current += 1;
			auditOpenRef.current = false;
			setAuditOpen(false);
			setAuditLoading(false);
			return;
		}
		const token = lifecycleTokenRef.current;
		if (token === null) return;
		auditOpenRef.current = true;
		setAuditOpen(true);
		setAuditRecords(null);
		loadAudit(token);
	}, [loadAudit]);

	const handleAuditRefresh = useCallback(() => {
		const token = lifecycleTokenRef.current;
		if (token === null || auditLoading || !auditOpenRef.current) return;
		loadAudit(token);
	}, [auditLoading, loadAudit]);

	const handleClose = useCallback(() => {
		clearControlExpiryTimer();
		setControlStatus({ armed: false, remainingMs: 0 });
		const lifecycleToken = lifecycleTokenRef.current;
		if (lifecycleToken === null) {
			onClose();
			return;
		}
		void hideBrowser({ workspaceId, sessionId, lifecycleToken }).catch(() => {}).finally(onClose);
	}, [clearControlExpiryTimer, onClose, sessionId, workspaceId]);

	const auditPopoverContent = (
		<PopoverContent side="bottom" align="end" className="w-80 max-w-[calc(100vw-1rem)] p-3">
			<div className="flex items-center justify-between gap-2">
				<div className="min-w-0">
					<p className="text-sm font-medium">{t("browser.audit.title")}</p>
					<p className="text-xs text-muted-foreground">{t("browser.audit.description")}</p>
				</div>
				<Button
					type="button"
					variant="ghost"
					size="icon-xs"
					onClick={handleAuditRefresh}
					aria-label={t("browser.audit.refresh")}
					title={t("browser.audit.refresh")}
					disabled={auditLoading}
				>
					{auditLoading ? <LoaderCircle className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />}
				</Button>
			</div>
			{auditLoading ? (
				<p className="flex items-center gap-1.5 py-4 text-xs text-muted-foreground" role="status">
					<LoaderCircle className="size-3 animate-spin" />
					{t("browser.audit.loading")}
				</p>
			) : auditFailed ? (
				<p className="py-4 text-xs text-destructive" role="alert">{t("browser.audit.error")}</p>
			) : auditRecords?.length ? (
				<ul className="max-h-72 space-y-1 overflow-y-auto" aria-label={t("browser.audit.entries")}>
					{auditRecords.map((record, index) => {
						const timestamp = browserAuditTime(record.timestampMs);
						return (
							<li key={`${record.timestampMs}-${record.tool}-${index}`} className="rounded-md border border-border/60 px-2 py-1.5 text-xs">
								<div className="flex items-center justify-between gap-2">
									<span className="min-w-0 truncate font-medium">{t(`browser.audit.tools.${record.tool}`)}</span>
									<time className="shrink-0 text-[10px] text-muted-foreground">
										{timestamp ? timestamp.toLocaleTimeString() : t("browser.audit.unknownTime")}
									</time>
								</div>
								<div className="mt-0.5 flex flex-wrap gap-x-1.5 gap-y-0.5 text-[10px] text-muted-foreground">
									<span>{record.origin === "mcp"
										? t("browser.audit.origin.mcp", { provider: record.providerId ?? t("browser.audit.origin.providerUnavailable") })
										: t("browser.audit.origin.ui")}</span>
									<span>{t(`browser.audit.grant.${record.grantState}`)}</span>
									<span>{t(`browser.audit.outcome.${record.outcome}`)}</span>
								</div>
							</li>
						);
					})}
				</ul>
			) : (
				<p className="py-4 text-xs text-muted-foreground">{t("browser.audit.empty")}</p>
			)}
		</PopoverContent>
	);

	const evidenceDisabled =
		!onSendEvidenceToAgent ||
		loading ||
		evidenceBusy ||
		evidenceCapture !== null ||
		!controlStatus.armed;

	return (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
			<header className="flex h-11 shrink-0 items-center gap-1.5 border-b border-border/60 bg-background/95 px-2.5 backdrop-blur">
				{/* The address bar is the primary object: navigation controls live inside it, like a real browser. */}
				<form
					className="relative flex min-w-0 flex-1 items-center"
					onSubmit={(event) => {
						event.preventDefault();
						handleNavigate();
					}}
				>
					<Globe2 className="pointer-events-none absolute left-2.5 size-3.5 shrink-0 text-cyan-400" aria-hidden />
					<input
						value={address}
						onChange={(event) => setAddress(event.target.value)}
						placeholder={t("browser.addressPlaceholder")}
						aria-label={t("browser.addressLabel")}
						enterKeyHint="go"
						className="h-7 w-full min-w-0 rounded-md border border-border/70 bg-muted/25 pr-8 pl-8 text-[12px] text-foreground outline-none placeholder:text-muted-foreground/60 focus:border-cyan-500/60 focus:ring-1 focus:ring-cyan-500/30"
					/>
					<Button
						type="button"
						variant="ghost"
						size="icon-xs"
						onClick={handleReload}
						aria-label={t("browser.reload")}
						title={t("browser.reload")}
						disabled={loading || lifecycleToken === null}
						className="absolute right-0.5 text-muted-foreground hover:text-foreground"
					>
						{loading ? <LoaderCircle className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />}
					</Button>
				</form>
				<Button
					type="button"
					variant="ghost"
					size="icon-sm"
					onClick={handleSendToAgent}
					aria-label={t("browser.sendToAgent")}
					title={t("browser.sendToAgent")}
					disabled={!onSendToAgent || loading || sendingContext}
				>
					{sendingContext ? (
						<LoaderCircle className="size-3.5 animate-spin" />
					) : (
						<Sparkles className="size-3.5" />
					)}
				</Button>
				{sessionId ? (
					// Everything the agent does with this page lives under one stateful pill.
					// The audit popover anchors to the same pill so the menu can hand off to it.
					<Popover open={auditOpen} onOpenChange={handleAuditOpenChange}>
						<DropdownMenu>
							<PopoverAnchor asChild>
								<DropdownMenuTrigger asChild>
									<Button
										type="button"
										variant="ghost"
										size="xs"
										aria-label={t("browser.agentMenu")}
										title={t("browser.agentMenu")}
										aria-busy={controlBusy}
										disabled={lifecycleToken === null}
										className={cn(
											"gap-1 px-1.5",
											controlStatus.armed && "text-emerald-400 hover:text-emerald-300",
										)}
									>
										{controlBusy ? (
											<LoaderCircle className="size-3.5 animate-spin" />
										) : controlStatus.armed ? (
											<ShieldCheck className="size-3.5" />
										) : (
											<Shield className="size-3.5" />
										)}
										<span className="tabular-nums">
											{controlStatus.armed
												? t("browser.controlCountdown", { seconds: controlSecondsLeft })
												: t("browser.agentLabel")}
										</span>
										<ChevronDown className="size-3 opacity-50" aria-hidden />
									</Button>
								</DropdownMenuTrigger>
							</PopoverAnchor>
							<DropdownMenuContent
								align="end"
								className="w-64"
								onCloseAutoFocus={(event) => {
									// Hand off to the audit popover only after the menu has closed;
									// returning focus to the pill first would count as focus-outside
									// for the popover and dismiss it immediately.
									if (!pendingAuditOpenRef.current) return;
									pendingAuditOpenRef.current = false;
									event.preventDefault();
									handleAuditOpenChange(true);
								}}
							>
								<DropdownMenuItem
									size="sm"
									onSelect={handleControlToggle}
									disabled={loading || controlBusy}
									className={controlStatus.armed ? "text-emerald-500 focus:text-emerald-500" : undefined}
								>
									{controlStatus.armed ? <ShieldCheck className="size-3.5" /> : <Shield className="size-3.5" />}
									{controlStatus.armed ? t("browser.disarmControl") : t("browser.armControl")}
								</DropdownMenuItem>
								<DropdownMenuItem
									size="sm"
									onSelect={handleStartEvidence}
									disabled={evidenceDisabled}
									className="flex-col items-start gap-0.5"
								>
									<span className="flex items-center gap-1">
										<Activity className="size-3.5" />
										{t("browser.evidence.start")}
									</span>
									{!controlStatus.armed ? (
										<span className="pl-4.5 text-[11px] text-muted-foreground">
											{t("browser.evidence.requiresControl")}
										</span>
									) : null}
								</DropdownMenuItem>
								<DropdownMenuSeparator />
								<DropdownMenuItem
									size="sm"
									onSelect={() => {
										pendingAuditOpenRef.current = true;
									}}
								>
									<History className="size-3.5" />
									{t("browser.audit.title")}
								</DropdownMenuItem>
							</DropdownMenuContent>
						</DropdownMenu>
						{auditPopoverContent}
					</Popover>
				) : (
					// Without a session there is no agent to arm; only the activity log remains.
					<Popover open={auditOpen} onOpenChange={handleAuditOpenChange}>
						<PopoverTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-sm"
								aria-label={t("browser.audit.open")}
								title={t("browser.audit.open")}
								disabled={lifecycleToken === null}
							>
								<History className="size-3.5" />
							</Button>
						</PopoverTrigger>
						{auditPopoverContent}
					</Popover>
				)}
				<Button type="button" variant="ghost" size="icon-sm" onClick={handleClose} aria-label={t("browser.close")} title={t("browser.close")}>
					<X className="size-3.5" />
				</Button>
			</header>
			<div className="flex min-h-7 shrink-0 items-center gap-2 border-b border-border/50 bg-background px-3 text-xs" aria-live="polite">
				{error ? (
					<span className="truncate text-destructive">{t("browser.error", { error })}</span>
				) : evidenceCapture ? (
					<span className="flex min-w-0 flex-1 items-center gap-2 text-cyan-600 dark:text-cyan-400">
						<Activity className="size-3 shrink-0" />
						<span className="truncate">
							{t("browser.evidence.active", { seconds: evidenceSecondsLeft })}
						</span>
						<Button
							type="button"
							variant="outline"
							size="xs"
							className="ml-auto h-5 shrink-0 px-2"
							onClick={handleCollectEvidence}
							disabled={evidenceBusy}
						>
							{t("browser.evidence.collect")}
						</Button>
						<Button
							type="button"
							variant="ghost"
							size="xs"
							className="h-5 shrink-0 px-2 text-muted-foreground"
							onClick={handleDiscardEvidence}
							disabled={evidenceBusy}
						>
							{t("browser.evidence.discard")}
						</Button>
					</span>
				) : loading ? (
					<span className="flex items-center gap-1.5 text-muted-foreground">
						<LoaderCircle className="size-3 animate-spin text-cyan-500" />
						{t("browser.loading")}
					</span>
				) : (
					<>
						{snapshot?.title ? (
							<span className="min-w-0 flex-1 truncate text-muted-foreground" title={snapshot.title}>
								{snapshot.title}
							</span>
						) : (
							<span className="flex-1" aria-hidden />
						)}
						{sessionId && controlStatus.armed ? (
							<span className="flex shrink-0 items-center gap-1 text-emerald-500">
								<ShieldCheck className="size-3" aria-hidden />
								{t("browser.controlActiveShort")}
							</span>
						) : null}
					</>
				)}
			</div>
			<div ref={viewportRef} className="relative min-h-0 min-w-0 flex-1 overflow-hidden bg-white">
			</div>
		</div>
	);
}
