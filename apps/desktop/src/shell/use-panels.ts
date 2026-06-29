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
	getInitialCollapsed,
	getInitialInspectorCollapsed,
	getInitialInspectorWidth,
	getInitialSidebarWidth,
	MAX_INSPECTOR_WIDTH,
	MAX_SIDEBAR_WIDTH,
	MIN_INSPECTOR_WIDTH,
	MIN_SIDEBAR_WIDTH,
	INSPECTOR_COLLAPSED_STORAGE_KEY,
	INSPECTOR_WIDTH_STORAGE_KEY,
	SIDEBAR_COLLAPSED_STORAGE_KEY,
	SIDEBAR_RESIZE_STEP,
	SIDEBAR_WIDTH_STORAGE_KEY,
} from "./layout";

type ResizeTarget = "sidebar" | "inspector";

type ResizeState = {
	pointerX: number;
	size: number;
	target: ResizeTarget;
};

function persistValue(storageKey: string, value: string) {
	try {
		window.localStorage.setItem(storageKey, value);
	} catch (error) {
		console.error(`[dcc] failed to persist "${storageKey}"`, error);
	}
}

function clampTargetWidth(target: ResizeTarget, width: number) {
	return target === "sidebar"
		? clampSidebarWidth(width)
		: clampInspectorWidth(width);
}

export function useShellPanels() {
	const [sidebarWidth, setSidebarWidth] = useState(getInitialSidebarWidth);
	const [sidebarCollapsed, setSidebarCollapsed] = useState(() =>
		getInitialCollapsed(SIDEBAR_COLLAPSED_STORAGE_KEY, false),
	);
	const [inspectorWidth, setInspectorWidth] = useState(() =>
		getInitialInspectorWidth(),
	);
	const [inspectorCollapsed, setInspectorCollapsed] = useState(
		getInitialInspectorCollapsed,
	);
	const [resizeState, setResizeState] = useState<ResizeState | null>(null);

	useEffect(() => {
		persistValue(SIDEBAR_WIDTH_STORAGE_KEY, String(sidebarWidth));
	}, [sidebarWidth]);

	useEffect(() => {
		persistValue(INSPECTOR_WIDTH_STORAGE_KEY, String(inspectorWidth));
	}, [inspectorWidth]);

	useEffect(() => {
		persistValue(SIDEBAR_COLLAPSED_STORAGE_KEY, String(sidebarCollapsed));
	}, [sidebarCollapsed]);

	useEffect(() => {
		persistValue(INSPECTOR_COLLAPSED_STORAGE_KEY, String(inspectorCollapsed));
	}, [inspectorCollapsed]);

	useEffect(() => {
		if (!resizeState) {
			return;
		}

		let rafId: number | null = null;
		let pendingWidth: number | null = null;

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

			pendingWidth = clampTargetWidth(resizeState.target, nextRawWidth);

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
			const currentWidth =
				target === "sidebar" ? sidebarWidth : inspectorWidth;

			let nextWidth = currentWidth;
			switch (event.key) {
				case "ArrowLeft":
					nextWidth = currentWidth - SIDEBAR_RESIZE_STEP;
					break;
				case "ArrowRight":
					nextWidth = currentWidth + SIDEBAR_RESIZE_STEP;
					break;
				case "Home":
					nextWidth =
						target === "sidebar" ? MIN_SIDEBAR_WIDTH : MIN_INSPECTOR_WIDTH;
					break;
				case "End":
					nextWidth =
						target === "sidebar" ? MAX_SIDEBAR_WIDTH : MAX_INSPECTOR_WIDTH;
					break;
				default:
					return;
			}

			event.preventDefault();
			const clamped = clampTargetWidth(target, nextWidth);
			if (target === "sidebar") {
				setSidebarWidth(clamped);
				return;
			}

			setInspectorWidth(clamped);
		},
		[inspectorWidth, sidebarWidth],
	);

	return {
		handleResizeKeyDown,
		handleResizeStart,
		inspectorCollapsed,
		inspectorWidth,
		isInspectorResizing: resizeState?.target === "inspector",
		isSidebarResizing: resizeState?.target === "sidebar",
		sidebarCollapsed,
		sidebarWidth,
		setInspectorCollapsed,
		setInspectorWidth,
		setSidebarCollapsed,
		setSidebarWidth,
	};
}
