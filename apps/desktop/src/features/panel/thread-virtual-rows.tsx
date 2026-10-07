import {
	defaultRangeExtractor,
	measureElement as measureVirtualElement,
	useVirtualizer,
	type Range,
} from "@tanstack/react-virtual";
import {
	useCallback,
	useImperativeHandle,
	useLayoutEffect,
	useRef,
	useState,
	type ReactNode,
	type RefObject,
} from "react";
import type { WorkspaceMessage } from "./thread-projection";

/** Rows drawn beyond each edge of the viewport. */
const THREAD_ROW_OVERSCAN = 6;
/** Rows the person opened or acted on stay mounted, up to this many. */
const PINNED_ROW_LIMIT = 24;
const ROW_SIZE_CACHE_LIMIT = 4000;
/** The top spacer above the rows; the real offset is measured once mounted. */
const LIST_TOP_GUESS_PX = 24;

/**
 * Measured heights by message id, kept across switches: a conversation
 * opened again lays out at its real height at once, so the restored
 * position and the scrollbar do not drift while rows are measured.
 */
const rowSizeCache = new Map<string, number>();

function rememberRowSize(id: string, size: number) {
	rowSizeCache.delete(id);
	rowSizeCache.set(id, size);
	if (rowSizeCache.size > ROW_SIZE_CACHE_LIMIT) {
		const oldest = rowSizeCache.keys().next().value;
		if (oldest !== undefined) rowSizeCache.delete(oldest);
	}
}

function lineCount(content: string): number {
	let lines = 1;
	for (let index = content.indexOf("\n"); index >= 0; index = content.indexOf("\n", index + 1)) {
		lines += 1;
	}
	return lines;
}

/**
 * A first guess before a row is measured, fitted on real conversations
 * (rendered markdown grows with both length and line breaks). Within a
 * session it is scaled by how measured rows compared to their guesses, which
 * absorbs the panel width.
 */
export function estimateThreadRowSize(message: WorkspaceMessage): number {
	const length = message.content.length;
	const lines = lineCount(message.content);
	if (message.role === "user") return Math.max(64, 55 + 0.084 * length + 22.6 * lines);
	if (message.role === "assistant") return Math.max(64, 37 + 0.326 * length + 17.3 * lines);
	return 64;
}

const estimateCalibration = { measured: 0, estimated: 0 };

function calibratedEstimate(message: WorkspaceMessage): number {
	const base = estimateThreadRowSize(message);
	if (estimateCalibration.estimated < 2000) return base;
	const scale = estimateCalibration.measured / estimateCalibration.estimated;
	return base * Math.min(2, Math.max(0.5, scale));
}

function calibrate(message: WorkspaceMessage, size: number) {
	estimateCalibration.measured += size;
	estimateCalibration.estimated += estimateThreadRowSize(message);
	// Recent rows weigh more (the panel may have been resized).
	if (estimateCalibration.estimated > 200_000) {
		estimateCalibration.measured /= 2;
		estimateCalibration.estimated /= 2;
	}
}

export type ThreadRowsLocator = {
	/** A row's top within the scroll content, or null when it is not in these rows. */
	offsetOf: (messageId: string) => number | null;
	/** Scrolls a row into view; false when it is not in these rows. */
	reveal: (messageId: string, align: "start" | "center", behavior: ScrollBehavior) => boolean;
};

/**
 * The settled part of a conversation, virtualized: only the rows near the
 * viewport are mounted. The rows keep their own markup (and the
 * `data-conversation-trail-id` the trail and find-in-thread look for); this
 * only positions them.
 */
export function ThreadVirtualRows({
	rows,
	renderRow,
	scrollRef,
	initialRow,
	locatorRef,
}: {
	rows: readonly WorkspaceMessage[];
	renderRow: (message: WorkspaceMessage, index: number) => ReactNode;
	scrollRef: RefObject<HTMLElement | null>;
	/** Where to open: a remembered row, or null for the end (the usual case). */
	initialRow: { rowId: string; offsetWithinRow: number } | null;
	locatorRef: RefObject<ThreadRowsLocator | null>;
}) {
	const listRef = useRef<HTMLDivElement | null>(null);
	const rowsRef = useRef(rows);
	rowsRef.current = rows;
	const [scrollMargin, setScrollMargin] = useState(0);
	const [pinned, setPinned] = useState<readonly string[]>([]);

	const listTop = useCallback(() => {
		const list = listRef.current;
		const scrollElement = scrollRef.current;
		if (!list || !scrollElement) return null;
		return (
			list.getBoundingClientRect().top -
			scrollElement.getBoundingClientRect().top +
			scrollElement.scrollTop
		);
	}, [scrollRef]);

	const pinnedIndexes = pinned
		.map((id) => rows.findIndex((row) => row.id === id))
		.filter((index) => index >= 0);
	const rangeExtractor = useCallback(
		(range: Range) => {
			const indexes = defaultRangeExtractor(range);
			if (pinnedIndexes.length === 0) return indexes;
			return [...new Set([...indexes, ...pinnedIndexes])].sort((left, right) => left - right);
		},
		// Pinned rows change rarely; the joined ids keep the extractor stable.
		// eslint-disable-next-line react-hooks/exhaustive-deps
		[pinnedIndexes.join(",")],
	);

	const virtualizer = useVirtualizer({
		count: rows.length,
		getScrollElement: () => scrollRef.current,
		getItemKey: (index) => rowsRef.current[index]?.id ?? index,
		estimateSize: (index) => {
			const row = rowsRef.current[index];
			if (!row) return 120;
			return rowSizeCache.get(row.id) ?? calibratedEstimate(row);
		},
		measureElement: (element, entry, instance) => {
			const size = measureVirtualElement(element, entry, instance);
			const id = element.getAttribute("data-thread-row-id");
			if (id && size > 0) {
				if (!rowSizeCache.has(id)) {
					const row = rowsRef.current.find((candidate) => candidate.id === id);
					if (row) calibrate(row, size);
				}
				rememberRowSize(id, size);
			}
			return size;
		},
		overscan: THREAD_ROW_OVERSCAN,
		scrollMargin,
		rangeExtractor,
		// Mount where the view will be, so the first rows drawn (and the size
		// corrections above them) belong there, not to the top of the list.
		initialOffset: () => {
			if (!initialRow) return Number.MAX_SAFE_INTEGER;
			const index = rowsRef.current.findIndex((row) => row.id === initialRow.rowId);
			if (index < 0) return Number.MAX_SAFE_INTEGER;
			let offset = LIST_TOP_GUESS_PX;
			for (let cursor = 0; cursor < index; cursor += 1) {
				const row = rowsRef.current[cursor]!;
				offset += rowSizeCache.get(row.id) ?? calibratedEstimate(row);
			}
			return offset + initialRow.offsetWithinRow;
		},
	});

	// Where this list starts inside the scroll content ("load earlier" and the
	// top spacer sit above it).
	useLayoutEffect(() => {
		const top = listTop();
		if (top !== null && Math.abs(top - scrollMargin) > 0.5) setScrollMargin(top);
	});

	useImperativeHandle(
		locatorRef,
		() => ({
			offsetOf: (messageId) => {
				const index = rowsRef.current.findIndex((row) => row.id === messageId);
				if (index < 0) return null;
				const item = virtualizer.measurementsCache[index];
				const top = listTop();
				if (!item || top === null) return null;
				return top + item.start - virtualizer.options.scrollMargin;
			},
			reveal: (messageId, align, behavior) => {
				const index = rowsRef.current.findIndex((row) => row.id === messageId);
				if (index < 0) return false;
				virtualizer.scrollToIndex(index, { align, behavior });
				return true;
			},
		}),
		[listTop, virtualizer],
	);

	const pin = useCallback((id: string) => {
		setPinned((current) =>
			current.includes(id) ? current : [...current, id].slice(-PINNED_ROW_LIMIT),
		);
	}, []);

	const items = virtualizer.getVirtualItems();
	const offset = virtualizer.options.scrollMargin;
	return (
		<div
			ref={listRef}
			className="dcc-thread-virtual-rows relative w-full"
			style={{ height: virtualizer.getTotalSize() }}
		>
			{items.map((item) => {
				const row = rows[item.index];
				if (!row) return null;
				return (
					<div
						key={item.key}
						ref={virtualizer.measureElement}
						data-index={item.index}
						data-thread-row-id={row.id}
						className="absolute left-0 top-0 w-full"
						style={{ transform: `translateY(${item.start - offset}px)` }}
						// Opening a fold or typing in a row keeps it mounted, so
						// scrolling away and back does not reset it.
						onPointerDownCapture={() => pin(row.id)}
						onKeyDownCapture={() => pin(row.id)}
					>
						{renderRow(row, item.index)}
					</div>
				);
			})}
		</div>
	);
}
