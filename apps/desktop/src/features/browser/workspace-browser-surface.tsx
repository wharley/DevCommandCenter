import { useCallback, useEffect, useRef, useState } from "react";
import { Globe2, History, LoaderCircle, RefreshCw, Shield, ShieldCheck, Sparkles, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
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
	type BrowserAgentContext,
	type BrowserAuditRecord,
	type BrowserBounds,
	type BrowserSnapshot,
} from "./browser-api";
import { isBrowserOccluded, useBrowserOcclusion } from "./browser-occlusion";

type WorkspaceBrowserSurfaceProps = {
	workspaceId: string;
	sessionId: string | null;
	onClose: () => void;
	onSendToAgent?: (context: BrowserAgentContext) => void;
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
		const url = address.trim();
		if (!url) return;
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

	return (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
			<header className="flex h-11 shrink-0 items-center gap-2 border-b border-border/60 bg-background/95 px-3 backdrop-blur">
				<Globe2 className="size-4 shrink-0 text-cyan-400" aria-hidden />
				<form
					className="flex min-w-0 flex-1 items-center gap-2"
					onSubmit={(event) => {
						event.preventDefault();
						handleNavigate();
					}}
				>
					<input
						value={address}
						onChange={(event) => setAddress(event.target.value)}
						placeholder={t("browser.addressPlaceholder")}
						aria-label={t("browser.addressLabel")}
						className="h-7 min-w-0 flex-1 rounded-md border border-border/70 bg-muted/25 px-2.5 text-[12px] text-foreground outline-none placeholder:text-muted-foreground/60 focus:border-cyan-500/60 focus:ring-1 focus:ring-cyan-500/30"
					/>
					<Button type="submit" variant="outline" size="xs" disabled={loading || !address.trim()}>
						{t("browser.go")}
					</Button>
				</form>
				{snapshot?.title ? (
					<span className="hidden max-w-48 truncate text-[11px] text-muted-foreground lg:block">
						{snapshot.title}
					</span>
				) : null}
				<Button type="button" variant="ghost" size="icon-sm" onClick={handleReload} aria-label={t("browser.reload")} disabled={loading}>
					<RefreshCw className="size-3.5" />
				</Button>
				<Button
					type="button"
					variant="ghost"
					size="icon-sm"
					onClick={handleSendToAgent}
					aria-label={t("browser.sendToAgent")}
					disabled={!onSendToAgent || loading || sendingContext}
				>
					{sendingContext ? (
						<LoaderCircle className="size-3.5 animate-spin" />
					) : (
						<Sparkles className="size-3.5" />
					)}
				</Button>
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
				</Popover>
				{sessionId ? (
					<Button
						type="button"
						variant="ghost"
						size="xs"
						onClick={handleControlToggle}
						aria-label={controlStatus.armed ? t("browser.disarmControl") : t("browser.armControl")}
						title={controlStatus.armed ? t("browser.disarmControl") : t("browser.armControl")}
						aria-pressed={controlStatus.armed}
						aria-busy={controlBusy}
						disabled={loading || controlBusy || lifecycleToken === null}
						className={controlStatus.armed ? "text-emerald-400 hover:text-emerald-300" : undefined}
					>
						{controlBusy ? <LoaderCircle className="size-3.5 animate-spin" /> : controlStatus.armed ? <ShieldCheck className="size-3.5" /> : <Shield className="size-3.5" />}
						<span>{controlStatus.armed ? t("browser.controlOnShort") : t("browser.armControlShort")}</span>
					</Button>
				) : null}
				<Button type="button" variant="ghost" size="icon-sm" onClick={handleClose} aria-label={t("browser.close")}>
					<X className="size-3.5" />
				</Button>
			</header>
			<div className="flex min-h-7 shrink-0 items-center border-b border-border/50 bg-background px-3 text-xs" aria-live="polite">
				{error ? (
					<span className="truncate text-destructive">{t("browser.error", { error })}</span>
				) : loading ? (
					<span className="flex items-center gap-1.5 text-muted-foreground">
						<LoaderCircle className="size-3 animate-spin text-cyan-500" />
						{t("browser.loading")}
					</span>
				) : sessionId ? (
					<span className={controlStatus.armed ? "text-emerald-500" : "text-muted-foreground"}>
						{controlStatus.armed
							? t("browser.controlActive")
							: t("browser.controlInactive")}
					</span>
				) : null}
			</div>
			<div ref={viewportRef} className="relative min-h-0 min-w-0 flex-1 overflow-hidden bg-white">
			</div>
		</div>
	);
}
