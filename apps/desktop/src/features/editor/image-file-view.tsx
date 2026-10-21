import { invoke } from "@tauri-apps/api/core";
import { useQuery } from "@tanstack/react-query";
import { ImageOff, Loader2, Maximize, Minus, Plus } from "lucide-react";
import {
	useCallback,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
	type PointerEvent as ReactPointerEvent,
} from "react";
import { useTranslation } from "react-i18next";
import { DCC_QUERY_GC_TIME_MS } from "@/lib/query-client";
import { cn } from "@/lib/utils";
import {
	clampImageZoom,
	dataUrlByteLength,
	fitImageZoom,
	formatImageBytes,
	stepImageZoom,
	workspaceAbsolutePath,
} from "./image-file-view.logic";

const PADDING = 24;
// Room under the image so the floating zoom bar never covers it.
const BOTTOM_PADDING = 56;

function useWorkspaceImage(absolutePath: string | null) {
	return useQuery({
		queryKey: ["workspaceImageFile", absolutePath],
		queryFn: async () => {
			const result = await invoke<{ dataUrl: string }>("read_image_preview", {
				path: absolutePath,
			});
			return result.dataUrl;
		},
		enabled: Boolean(absolutePath),
		// The agent may regenerate a screenshot; reopening the tab rereads it.
		staleTime: 15_000,
		gcTime: DCC_QUERY_GC_TIME_MS.heavyPayload,
		refetchOnWindowFocus: false,
		retry: false,
	});
}

type Zoom = { mode: "fit" } | { mode: "manual"; value: number };
type Anchor = { viewX: number; viewY: number; imageX: number; imageY: number };

/** A workspace image opened as a tab: fits the view, zooms and pans. */
export function ImageFileView({
	workspaceRoot,
	path,
}: {
	workspaceRoot: string | null;
	path: string;
}) {
	const { t } = useTranslation("common");
	const absolutePath = workspaceRoot
		? workspaceAbsolutePath(workspaceRoot, path)
		: null;
	const image = useWorkspaceImage(absolutePath);
	const scrollerRef = useRef<HTMLDivElement | null>(null);
	const [natural, setNatural] = useState<{
		width: number;
		height: number;
	} | null>(null);
	const [box, setBox] = useState({ width: 0, height: 0 });
	const [zoom, setZoom] = useState<Zoom>({ mode: "fit" });
	const anchorRef = useRef<Anchor | null>(null);
	const dragRef = useRef<{
		x: number;
		y: number;
		left: number;
		top: number;
	} | null>(null);
	const [dragging, setDragging] = useState(false);

	useEffect(() => {
		setZoom({ mode: "fit" });
		setNatural(null);
	}, [image.data]);

	useEffect(() => {
		const el = scrollerRef.current;
		if (!el) return;
		const measure = () => setBox({ width: el.clientWidth, height: el.clientHeight });
		measure();
		const observer = new ResizeObserver(measure);
		observer.observe(el);
		return () => observer.disconnect();
	}, []);

	const fit = natural
		? fitImageZoom(natural, {
				width: box.width - PADDING * 2,
				height: box.height - PADDING - BOTTOM_PADDING,
			})
		: 1;
	const scale = zoom.mode === "fit" ? fit : zoom.value;

	// Zoom around a point (cursor or view center) so what you look at stays put.
	const zoomTo = useCallback(
		(next: number, clientPoint?: { x: number; y: number }) => {
			const el = scrollerRef.current;
			const target = clampImageZoom(next);
			if (el) {
				const rect = el.getBoundingClientRect();
				const x = clientPoint ? clientPoint.x - rect.left : el.clientWidth / 2;
				const y = clientPoint ? clientPoint.y - rect.top : el.clientHeight / 2;
				anchorRef.current = {
					viewX: x,
					viewY: y,
					// Point under the cursor, in image pixels.
					imageX: (el.scrollLeft + x - PADDING) / scale,
					imageY: (el.scrollTop + y - PADDING) / scale,
				};
			}
			setZoom({ mode: "manual", value: target });
		},
		[scale],
	);

	useLayoutEffect(() => {
		const el = scrollerRef.current;
		const anchor = anchorRef.current;
		anchorRef.current = null;
		if (!el || !anchor) return;
		el.scrollLeft = anchor.imageX * scale + PADDING - anchor.viewX;
		el.scrollTop = anchor.imageY * scale + PADDING - anchor.viewY;
	}, [scale]);

	// Pinch on the trackpad and ⌘/Ctrl + scroll arrive as wheel events with ctrlKey.
	useEffect(() => {
		const el = scrollerRef.current;
		if (!el) return;
		const onWheel = (event: WheelEvent) => {
			if (!event.ctrlKey && !event.metaKey) return;
			event.preventDefault();
			// Trackpad pinch sends small deltas; a mouse wheel notch is ~100.
			const delta = Math.max(-50, Math.min(50, event.deltaY));
			const factor = Math.exp(-delta * 0.006);
			zoomTo(scale * factor, { x: event.clientX, y: event.clientY });
		};
		el.addEventListener("wheel", onWheel, { passive: false });
		return () => el.removeEventListener("wheel", onWheel);
	}, [scale, zoomTo]);

	const overflows = natural
		? natural.width * scale + PADDING * 2 > box.width ||
			natural.height * scale + PADDING + BOTTOM_PADDING > box.height
		: false;

	const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
		const el = scrollerRef.current;
		if (!el || !overflows || event.button !== 0) return;
		dragRef.current = {
			x: event.clientX,
			y: event.clientY,
			left: el.scrollLeft,
			top: el.scrollTop,
		};
		setDragging(true);
		el.setPointerCapture(event.pointerId);
	};
	const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
		const el = scrollerRef.current;
		const drag = dragRef.current;
		if (!el || !drag) return;
		el.scrollLeft = drag.left - (event.clientX - drag.x);
		el.scrollTop = drag.top - (event.clientY - drag.y);
	};
	const endDrag = () => {
		dragRef.current = null;
		setDragging(false);
	};

	const onDoubleClick = (event: React.MouseEvent) => {
		if (zoom.mode === "fit" && Math.abs(fit - 1) > 0.01) {
			zoomTo(1, { x: event.clientX, y: event.clientY });
		} else if (zoom.mode === "manual" && Math.abs(zoom.value - 1) < 0.01) {
			setZoom({ mode: "fit" });
		} else {
			zoomTo(1, { x: event.clientX, y: event.clientY });
		}
	};

	const bytes = image.data ? dataUrlByteLength(image.data) : 0;

	return (
		<div className="relative flex h-full min-h-0 flex-col bg-background">
			<div
				ref={scrollerRef}
				onPointerDown={onPointerDown}
				onPointerMove={onPointerMove}
				onPointerUp={endDrag}
				onPointerCancel={endDrag}
				onDoubleClick={image.data ? onDoubleClick : undefined}
				className={cn(
					"min-h-0 flex-1 overflow-auto",
					overflows && (dragging ? "cursor-grabbing" : "cursor-grab"),
				)}
			>
				{image.data ? (
					<div
						className="flex min-h-full min-w-full items-center justify-center"
						style={{
							padding: `${PADDING}px ${PADDING}px ${BOTTOM_PADDING}px`,
							width: "max-content",
							height: "max-content",
						}}
					>
						<img
							src={image.data}
							alt={path}
							draggable={false}
							onLoad={(event) => {
								const el = scrollerRef.current;
								if (el) setBox({ width: el.clientWidth, height: el.clientHeight });
								setNatural({
									width: event.currentTarget.naturalWidth,
									height: event.currentTarget.naturalHeight,
								});
							}}
							className={cn(
								"block max-w-none select-none shadow-sm ring-1 ring-border/60",
								"bg-[length:16px_16px] bg-[position:0_0,8px_8px]",
								"bg-[image:linear-gradient(45deg,var(--muted)_25%,transparent_25%,transparent_75%,var(--muted)_75%),linear-gradient(45deg,var(--muted)_25%,transparent_25%,transparent_75%,var(--muted)_75%)]",
								!natural && "invisible",
							)}
							style={
								natural
									? {
											width: natural.width * scale,
											height: natural.height * scale,
											imageRendering: scale >= 2 ? "pixelated" : "auto",
										}
									: undefined
							}
						/>
					</div>
				) : image.isError ? (
					<div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center text-sm text-muted-foreground">
						<ImageOff className="size-5" strokeWidth={1.6} />
						<span>{t("imageView.unavailable")}</span>
					</div>
				) : (
					<div className="flex h-full items-center justify-center text-muted-foreground">
						<Loader2 className="size-4 animate-spin" strokeWidth={1.8} />
					</div>
				)}
			</div>
			{image.data && natural ? (
				<div className="pointer-events-none absolute inset-x-0 bottom-3 flex justify-center">
					<div className="pointer-events-auto flex items-center gap-0.5 rounded-lg border border-border bg-background/95 px-1 py-0.5 text-[11px] text-muted-foreground shadow-sm backdrop-blur">
						<ZoomButton
							label={t("imageView.zoomOut")}
							onClick={() => zoomTo(stepImageZoom(scale, -1))}
						>
							<Minus className="size-3.5" strokeWidth={1.8} />
						</ZoomButton>
						<button
							type="button"
							onClick={() => zoomTo(1)}
							title={t("imageView.actualSize")}
							className="min-w-12 rounded px-1.5 py-1 tabular-nums hover:bg-muted/60 hover:text-foreground"
						>
							{Math.round(scale * 100)}%
						</button>
						<ZoomButton
							label={t("imageView.zoomIn")}
							onClick={() => zoomTo(stepImageZoom(scale, 1))}
						>
							<Plus className="size-3.5" strokeWidth={1.8} />
						</ZoomButton>
						<ZoomButton
							label={t("imageView.fit")}
							active={zoom.mode === "fit"}
							onClick={() => setZoom({ mode: "fit" })}
						>
							<Maximize className="size-3.5" strokeWidth={1.8} />
						</ZoomButton>
						<span className="ml-1 border-l border-border pl-2 pr-1.5 tabular-nums">
							{natural.width} × {natural.height} · {formatImageBytes(bytes)}
						</span>
					</div>
				</div>
			) : null}
		</div>
	);
}

function ZoomButton({
	label,
	active,
	onClick,
	children,
}: {
	label: string;
	active?: boolean;
	onClick: () => void;
	children: React.ReactNode;
}) {
	return (
		<button
			type="button"
			aria-label={label}
			title={label}
			onClick={onClick}
			className={cn(
				"rounded p-1 hover:bg-muted/60 hover:text-foreground",
				active && "bg-muted/60 text-foreground",
			)}
		>
			{children}
		</button>
	);
}
