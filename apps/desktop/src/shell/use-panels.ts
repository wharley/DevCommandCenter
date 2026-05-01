import {
	type KeyboardEvent,
	type MouseEvent,
	useCallback,
	useEffect,
	useState,
} from "react";
import {
	clampInspectorWidth,
	clampSidebarWidth,
	getInitialInspectorWidth,
	getInitialSidebarWidth,
	INSPECTOR_WIDTH_STORAGE_KEY,
	SIDEBAR_RESIZE_STEP,
	SIDEBAR_WIDTH_STORAGE_KEY,
} from "./layout";

type ResizeTarget = "sidebar" | "inspector";

type ResizeState = {
	pointerX: number;
	size: number;
	target: ResizeTarget;
};

function persistWidth(storageKey: string, width: number) {
	try {
		window.localStorage.setItem(storageKey, String(width));
	} catch (error) {
		console.error(`[dcc] failed to persist "${storageKey}"`, error);
	}
}

export function useShellPanels() {
	const [sidebarWidth, setSidebarWidth] = useState(getInitialSidebarWidth);
	const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
	const [inspectorWidth, setInspectorWidth] = useState(() =>
		getInitialInspectorWidth(),
	);
	const [resizeState, setResizeState] = useState<ResizeState | null>(null);

	useEffect(() => {
		persistWidth(SIDEBAR_WIDTH_STORAGE_KEY, sidebarWidth);
	}, [sidebarWidth]);

	useEffect(() => {
		persistWidth(INSPECTOR_WIDTH_STORAGE_KEY, inspectorWidth);
	}, [inspectorWidth]);

	useEffect(() => {
		if (!resizeState) {
			return;
		}

		let pendingWidth: number | null = null;
		let rafId: number | null = null;

		const flush = () => {
			rafId = null;
			if (pendingWidth === null) {
				return;
			}

			const nextWidth = pendingWidth;
			pendingWidth = null;

			if (resizeState.target === "sidebar") {
				setSidebarWidth(nextWidth);
			} else {
				setInspectorWidth(nextWidth);
			}
		};

		const handleMouseMove = (event: globalThis.MouseEvent) => {
			const deltaX = event.clientX - resizeState.pointerX;
			const nextRawWidth =
				resizeState.target === "sidebar"
					? resizeState.size + deltaX
					: resizeState.size - deltaX;

			pendingWidth =
				resizeState.target === "sidebar"
					? clampSidebarWidth(nextRawWidth)
					: clampInspectorWidth(nextRawWidth);

			if (rafId === null) {
				rafId = window.requestAnimationFrame(flush);
			}
		};

		const handleMouseUp = () => {
			if (rafId !== null) {
				window.cancelAnimationFrame(rafId);
			}
			flush();
			setResizeState(null);
		};

		const previousCursor = document.body.style.cursor;
		const previousUserSelect = document.body.style.userSelect;

		document.body.style.cursor = "ew-resize";
		document.body.style.userSelect = "none";

		window.addEventListener("mousemove", handleMouseMove);
		window.addEventListener("mouseup", handleMouseUp);

		return () => {
			if (rafId !== null) {
				window.cancelAnimationFrame(rafId);
			}
			document.body.style.cursor = previousCursor;
			document.body.style.userSelect = previousUserSelect;
			window.removeEventListener("mousemove", handleMouseMove);
			window.removeEventListener("mouseup", handleMouseUp);
		};
	}, [resizeState]);

	const handleResizeStart = useCallback(
		(target: ResizeTarget) => (event: MouseEvent<HTMLDivElement>) => {
			event.preventDefault();
			setResizeState({
				pointerX: event.clientX,
				size: target === "sidebar" ? sidebarWidth : inspectorWidth,
				target,
			});
		},
		[inspectorWidth, sidebarWidth],
	);

	const handleResizeKeyDown = useCallback(
		(target: ResizeTarget) => (event: KeyboardEvent<HTMLDivElement>) => {
			if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") {
				return;
			}

			event.preventDefault();
			const delta = event.key === "ArrowRight" ? SIDEBAR_RESIZE_STEP : -SIDEBAR_RESIZE_STEP;

			if (target === "sidebar") {
				setSidebarWidth((currentWidth) => clampSidebarWidth(currentWidth + delta));
				return;
			}

			setInspectorWidth((currentWidth) => clampInspectorWidth(currentWidth - delta));
		},
		[],
	);

	return {
		handleResizeKeyDown,
		handleResizeStart,
		inspectorWidth,
		isInspectorResizing: resizeState?.target === "inspector",
		isSidebarResizing: resizeState?.target === "sidebar",
		sidebarCollapsed,
		sidebarWidth,
		setInspectorWidth,
		setSidebarCollapsed,
		setSidebarWidth,
	};
}
