import { ChevronLeft, ChevronRight } from "lucide-react";
import {
	useCallback,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
	type ReactNode,
	type WheelEvent,
} from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import {
	arrowScrollStep,
	scrollDeltaToReveal,
	TAB_STRIP_EDGE_INSET,
	tabStripOverflow,
	type TabStripOverflow,
} from "./scrollable-tab-strip.logic";

const FADE = TAB_STRIP_EDGE_INSET;

/**
 * Tab strip for the stage. No scrollbar: when tabs overflow, the hidden side
 * fades out and shows an arrow, the mouse wheel scrolls sideways, and the
 * selected tab is kept in view.
 */
export function ScrollableTabStrip({
	revealKey,
	ariaLabel,
	children,
}: {
	/** Changes whenever another tab is selected or opened. */
	revealKey: string;
	ariaLabel?: string;
	children: ReactNode;
}) {
	const { t } = useTranslation("common");
	const scrollerRef = useRef<HTMLDivElement>(null);
	const [overflow, setOverflow] = useState<TabStripOverflow>({
		left: false,
		right: false,
	});

	const measure = useCallback(() => {
		const el = scrollerRef.current;
		if (!el) {
			return;
		}
		const next = tabStripOverflow(el.scrollLeft, el.clientWidth, el.scrollWidth);
		setOverflow((prev) =>
			prev.left === next.left && prev.right === next.right ? prev : next,
		);
	}, []);

	// Tabs open and close on any render; measuring is cheap.
	useLayoutEffect(measure);

	useEffect(() => {
		const el = scrollerRef.current;
		if (!el || typeof ResizeObserver === "undefined") {
			return;
		}
		const observer = new ResizeObserver(measure);
		observer.observe(el);
		return () => observer.disconnect();
	}, [measure]);

	useLayoutEffect(() => {
		const el = scrollerRef.current;
		const tab = el?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
		if (!el || !tab) {
			return;
		}
		const delta = scrollDeltaToReveal(
			tab.offsetLeft,
			tab.offsetLeft + tab.offsetWidth,
			el.scrollLeft,
			el.clientWidth,
		);
		if (delta !== 0) {
			el.scrollBy({ left: delta, behavior: "smooth" });
		}
	}, [revealKey]);

	const handleWheel = (event: WheelEvent<HTMLDivElement>) => {
		const el = event.currentTarget;
		if (
			el.scrollWidth <= el.clientWidth ||
			Math.abs(event.deltaY) <= Math.abs(event.deltaX)
		) {
			return;
		}
		// Line-based wheels (deltaMode 1) report rows, not pixels.
		el.scrollLeft += event.deltaMode === 1 ? event.deltaY * 16 : event.deltaY;
	};

	const scrollByArrow = (direction: -1 | 1) => {
		const el = scrollerRef.current;
		if (el) {
			el.scrollBy({
				left: direction * arrowScrollStep(el.clientWidth),
				behavior: "smooth",
			});
		}
	};

	const mask =
		overflow.left || overflow.right
			? `linear-gradient(to right, transparent 0, #000 ${overflow.left ? FADE : 0}px, #000 calc(100% - ${overflow.right ? FADE : 0}px), transparent 100%)`
			: undefined;

	// The strip itself is `relative`, so a tab's offsetLeft is its position in
	// the scrolled content.
	return (
		<div className="relative flex h-full min-w-0 flex-1 items-center">
			<div
				ref={scrollerRef}
				role="tablist"
				aria-label={ariaLabel}
				onScroll={measure}
				onWheel={handleWheel}
				style={{ maskImage: mask, WebkitMaskImage: mask }}
				className="no-scrollbar relative flex h-full min-w-0 flex-1 items-center gap-0.5 overflow-x-auto overscroll-x-contain px-2"
			>
				{children}
			</div>
			{overflow.left ? (
				<EdgeArrow
					side="left"
					label={t("fileTabs.scrollLeft")}
					onClick={() => scrollByArrow(-1)}
				/>
			) : null}
			{overflow.right ? (
				<EdgeArrow
					side="right"
					label={t("fileTabs.scrollRight")}
					onClick={() => scrollByArrow(1)}
				/>
			) : null}
		</div>
	);
}

function EdgeArrow({
	side,
	label,
	onClick,
}: {
	side: "left" | "right";
	label: string;
	onClick: () => void;
}) {
	const Icon = side === "left" ? ChevronLeft : ChevronRight;
	return (
		<button
			type="button"
			aria-label={label}
			title={label}
			tabIndex={-1}
			onClick={onClick}
			className={cn(
				"absolute top-1/2 z-10 flex size-6 -translate-y-1/2 items-center justify-center rounded-md bg-background text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground",
				side === "left" ? "left-0.5" : "right-0.5",
			)}
		>
			<Icon className="size-3.5" strokeWidth={2} />
		</button>
	);
}
