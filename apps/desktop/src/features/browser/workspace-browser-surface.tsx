import { useCallback, useEffect, useRef, useState } from "react";
import { Globe2, LoaderCircle, RefreshCw, Shield, ShieldCheck, Sparkles, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	hideBrowser,
	getBrowserControlStatus,
	armBrowserControl,
	disarmBrowserControl,
	listenBrowserState,
	navigateBrowser,
	openBrowser,
	reloadBrowser,
	setBrowserBounds,
	extractBrowserContext,
	type BrowserAgentContext,
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
	const [lifecycleToken, setLifecycleToken] = useState<number | null>(null);
	const lifecycleTokenRef = useRef<number | null>(null);
	const controlExpiryTimerRef = useRef<number | null>(null);
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
		void openBrowser({ workspaceId, sessionId, bounds, initialOccluded: isBrowserOccluded(viewport) })
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
