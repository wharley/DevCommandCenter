import { useCallback, useEffect, useRef, useState } from "react";
import { Globe2, LoaderCircle, RefreshCw, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	hideBrowser,
	listenBrowserState,
	navigateBrowser,
	openBrowser,
	reloadBrowser,
	setBrowserBounds,
	type BrowserBounds,
	type BrowserSnapshot,
} from "./browser-api";

type WorkspaceBrowserSurfaceProps = {
	workspaceId: string;
	sessionId: string | null;
	onClose: () => void;
};

export function readBrowserBounds(element: HTMLElement): BrowserBounds {
	const rect = element.getBoundingClientRect();
	return {
		x: Math.max(0, rect.left),
		y: Math.max(0, rect.top),
		width: Math.max(1, rect.width),
		height: Math.max(1, rect.height),
	};
}

export function WorkspaceBrowserSurface({
	workspaceId,
	sessionId,
	onClose,
}: WorkspaceBrowserSurfaceProps) {
	const viewportRef = useRef<HTMLDivElement | null>(null);
	const boundsFrameRef = useRef<number | null>(null);
	const { t } = useTranslation("common");
	const [snapshot, setSnapshot] = useState<BrowserSnapshot | null>(null);
	const [address, setAddress] = useState("");
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);

	const updateBounds = useCallback(() => {
		const viewport = viewportRef.current;
		if (!viewport) return;
		void setBrowserBounds({ workspaceId, sessionId, bounds: readBrowserBounds(viewport) }).catch(() => {
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
		void openBrowser({ workspaceId, sessionId, bounds })
			.then((next) => {
				if (cancelled) return;
				setSnapshot(next);
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
		void listenBrowserState((next) => {
			if (next.workspaceId !== workspaceId || next.sessionId !== sessionId) return;
			setSnapshot(next);
			if (next.url) setAddress(next.url);
			setLoading(false);
		})
			.then((dispose) => {
				unlisten = dispose;
			})
			.catch(() => {});
		return () => unlisten?.();
	}, [sessionId, workspaceId]);

	useEffect(() => {
		return () => {
			void hideBrowser({ workspaceId, sessionId }).catch(() => {});
		};
	}, [sessionId, workspaceId]);

	useEffect(() => {
		const viewport = viewportRef.current;
		if (!viewport || typeof ResizeObserver === "undefined") return;
		const observer = new ResizeObserver(scheduleBoundsUpdate);
		observer.observe(viewport);
		return () => {
			observer.disconnect();
			if (boundsFrameRef.current !== null) {
				cancelAnimationFrame(boundsFrameRef.current);
				boundsFrameRef.current = null;
			}
		};
	}, [scheduleBoundsUpdate]);

	const handleNavigate = useCallback(() => {
		const url = address.trim();
		if (!url) return;
		setError(null);
		setLoading(true);
		void navigateBrowser({ workspaceId, sessionId, url })
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
		setError(null);
		setLoading(true);
		void reloadBrowser({ workspaceId, sessionId })
			.catch((reason: unknown) => {
				setError(reason instanceof Error ? reason.message : String(reason));
			})
			.finally(() => setLoading(false));
	}, [sessionId, workspaceId]);

	const handleClose = useCallback(() => {
		void hideBrowser({ workspaceId, sessionId }).catch(() => {}).finally(onClose);
	}, [onClose, sessionId, workspaceId]);

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
				) : null}
			</div>
			<div ref={viewportRef} className="relative min-h-0 min-w-0 flex-1 overflow-hidden bg-white">
			</div>
		</div>
	);
}
