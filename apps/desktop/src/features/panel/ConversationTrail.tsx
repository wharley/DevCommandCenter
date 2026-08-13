import {
	useCallback,
	useEffect,
	useId,
	useMemo,
	useRef,
	useState,
	type FocusEvent,
	type KeyboardEvent,
	type PointerEvent,
	type RefObject,
} from "react";
import { useTranslation } from "react-i18next";
import type { WorkspaceMessage } from "./thread-projection";
import {
	deriveConversationTrailItems,
	focusedTrailIndex,
	trailMagnificationWeights,
} from "./conversation-trail.logic";

const TICK_SPACING_PX = 10;
const TICK_BASE_WIDTH_PX = 6;
const TICK_MAX_WIDTH_PX = 30;
const TICK_SIGMA_PX = 14;

type ConversationTrailProps = {
	messages: readonly WorkspaceMessage[];
	scrollRef: RefObject<HTMLElement | null>;
};

function findMessageElement(
	scrollElement: HTMLElement,
	messageId: string,
): HTMLElement | null {
	return (
		Array.from(
			scrollElement.querySelectorAll<HTMLElement>("[data-conversation-trail-id]"),
		).find((element) => element.dataset.conversationTrailId === messageId) ?? null
	);
}

export function ConversationTrail({ messages, scrollRef }: ConversationTrailProps) {
	const { t } = useTranslation("common");
	const tooltipId = useId();
	const trailItems = useMemo(
		() => deriveConversationTrailItems(messages),
		[messages],
	);
	const rootRef = useRef<HTMLElement | null>(null);
	const viewportRef = useRef<HTMLDivElement | null>(null);
	const frameRef = useRef<number | null>(null);
	const pointerClientYRef = useRef<number | null>(null);
	const trailItemsRef = useRef(trailItems);
	trailItemsRef.current = trailItems;
	const [currentMessageId, setCurrentMessageId] = useState<string | null>(
		trailItems[0]?.id ?? null,
	);
	const [pointerY, setPointerY] = useState<number | null>(null);
	const [keyboardIndex, setKeyboardIndex] = useState<number | null>(null);
	const [rovingIndex, setRovingIndex] = useState(0);
	const currentTrailIndex = trailItems.findIndex(
		(item) => item.id === currentMessageId,
	);

	const updateCurrentMessage = useCallback(() => {
		frameRef.current = null;
		const scrollElement = scrollRef.current;
		const currentTrailItems = trailItemsRef.current;
		if (!scrollElement || currentTrailItems.length === 0) {
			return;
		}

		const viewportTop = scrollElement.getBoundingClientRect().top + 32;
		let nextId = currentTrailItems[0]!.id;
		for (const item of currentTrailItems) {
			const element = findMessageElement(scrollElement, item.id);
			if (!element || element.getBoundingClientRect().top > viewportTop) {
				break;
			}
			nextId = item.id;
		}
		setCurrentMessageId((current) => (current === nextId ? current : nextId));
	}, [scrollRef]);

	useEffect(() => {
		const scrollElement = scrollRef.current;
		if (!scrollElement || trailItems.length < 2) {
			return;
		}
		const scheduleUpdate = () => {
			if (frameRef.current === null) {
				frameRef.current = requestAnimationFrame(updateCurrentMessage);
			}
		};
		scheduleUpdate();
		scrollElement.addEventListener("scroll", scheduleUpdate, { passive: true });
		const resizeObserver = new ResizeObserver(scheduleUpdate);
		resizeObserver.observe(scrollElement);
		return () => {
			scrollElement.removeEventListener("scroll", scheduleUpdate);
			resizeObserver.disconnect();
			if (frameRef.current !== null) {
				cancelAnimationFrame(frameRef.current);
				frameRef.current = null;
			}
		};
	}, [scrollRef, trailItems.length, updateCurrentMessage]);

	useEffect(() => {
		setCurrentMessageId(trailItems[0]?.id ?? null);
		setPointerY(null);
		pointerClientYRef.current = null;
		setKeyboardIndex(null);
		setRovingIndex(0);
	}, [trailItems[0]?.id]);

	useEffect(() => {
		const viewport = viewportRef.current;
		if (!viewport || currentTrailIndex < 0) {
			return;
		}
		const tickTop = currentTrailIndex * TICK_SPACING_PX;
		if (tickTop < viewport.scrollTop) {
			viewport.scrollTop = tickTop;
		} else if (tickTop + 2 > viewport.scrollTop + viewport.clientHeight) {
			viewport.scrollTop = tickTop + 2 - viewport.clientHeight;
		}
	}, [currentTrailIndex]);

	if (trailItems.length < 2) {
		return null;
	}

	const activeIndex =
		pointerY === null
			? keyboardIndex
			: focusedTrailIndex(pointerY, trailItems.length, TICK_SPACING_PX);
	const reduceMotion =
		typeof window !== "undefined" &&
		window.matchMedia("(prefers-reduced-motion: reduce)").matches;
	const weights =
		activeIndex === null
			? trailItems.map(() => 0)
			: reduceMotion
				? trailItems.map((_, index) => (index === activeIndex ? 1 : 0))
				: trailMagnificationWeights(
						trailItems.length,
						pointerY ?? activeIndex * TICK_SPACING_PX,
						TICK_SPACING_PX,
						TICK_SIGMA_PX,
					);
	const activeItem = activeIndex === null ? null : trailItems[activeIndex] ?? null;
	const trackHeight = (trailItems.length - 1) * TICK_SPACING_PX + 2;

	const navigateTo = (index: number) => {
		const item = trailItems[index];
		const scrollElement = scrollRef.current;
		if (!item || !scrollElement) {
			return;
		}
		findMessageElement(scrollElement, item.id)?.scrollIntoView({
			behavior: reduceMotion ? "auto" : "smooth",
			block: "start",
		});
	};

	const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
		let nextIndex = rovingIndex;
		switch (event.key) {
			case "ArrowDown":
				nextIndex = Math.min(trailItems.length - 1, rovingIndex + 1);
				break;
			case "ArrowUp":
				nextIndex = Math.max(0, rovingIndex - 1);
				break;
			case "Home":
				nextIndex = 0;
				break;
			case "End":
				nextIndex = trailItems.length - 1;
				break;
			case "Enter":
			case " ":
				event.preventDefault();
				navigateTo(rovingIndex);
				return;
			case "Escape":
				(document.activeElement as HTMLElement | null)?.blur();
				return;
			default:
				return;
		}
		event.preventDefault();
		setRovingIndex(nextIndex);
		viewportRef.current
			?.querySelectorAll<HTMLButtonElement>("button")
			.item(nextIndex)
			.focus();
	};

	const updatePointer = (event: PointerEvent<HTMLDivElement>) => {
		if (event.pointerType === "touch") {
			return;
		}
		const viewport = viewportRef.current;
		if (!viewport) {
			return;
		}
		pointerClientYRef.current = event.clientY;
		setPointerY(
			event.clientY - viewport.getBoundingClientRect().top + viewport.scrollTop,
		);
	};

	const handleBlur = (event: FocusEvent<HTMLElement>) => {
		if (
			event.relatedTarget instanceof Node &&
			event.currentTarget.contains(event.relatedTarget)
		) {
			return;
		}
		setKeyboardIndex(null);
	};

	return (
		<nav
			ref={rootRef}
			aria-label={t("conversation.trail.label")}
			onKeyDown={handleKeyDown}
			onBlur={handleBlur}
			className="dcc-conversation-trail"
		>
			<div
				ref={viewportRef}
				className="dcc-conversation-trail__viewport"
				onPointerEnter={updatePointer}
				onPointerMove={updatePointer}
				onPointerLeave={(event) => {
					if (event.pointerType !== "touch") {
						pointerClientYRef.current = null;
						setPointerY(null);
					}
				}}
				onScroll={(event) => {
					if (pointerClientYRef.current !== null) {
						const viewport = event.currentTarget;
						setPointerY(
							pointerClientYRef.current -
								viewport.getBoundingClientRect().top +
								viewport.scrollTop,
						);
					}
				}}
				onClick={(event) => {
					if (event.target instanceof HTMLButtonElement) {
						return;
					}
					const viewport = event.currentTarget;
					const clickY =
						event.clientY - viewport.getBoundingClientRect().top + viewport.scrollTop;
					navigateTo(
						focusedTrailIndex(clickY, trailItems.length, TICK_SPACING_PX),
					);
				}}
			>
				<div className="dcc-conversation-trail__track" style={{ height: trackHeight }}>
					{trailItems.map((item, index) => {
						const isCurrent = item.id === currentMessageId;
						const isFocused = index === activeIndex;
						const width =
							TICK_BASE_WIDTH_PX +
							(TICK_MAX_WIDTH_PX - TICK_BASE_WIDTH_PX) * (weights[index] ?? 0);
						return (
							<button
								key={item.id}
								type="button"
								tabIndex={index === rovingIndex ? 0 : -1}
								aria-current={isCurrent ? "location" : undefined}
								aria-describedby={isFocused ? tooltipId : undefined}
								aria-label={t("conversation.trail.itemLabel", {
									position: item.ordinal,
									preview: item.promptPreview.slice(0, 80),
								})}
								onClick={() => navigateTo(index)}
								onFocus={() => {
									setRovingIndex(index);
									setKeyboardIndex(index);
								}}
								className="dcc-conversation-trail__tick"
								style={{
									top: index * TICK_SPACING_PX,
									width,
									opacity: isFocused ? 1 : isCurrent ? 0.9 : 0.24,
								}}
							/>
						);
					})}
				</div>
			</div>
			<div
				id={tooltipId}
				role="tooltip"
				aria-hidden={!activeItem}
				className="dcc-conversation-trail__tooltip"
				style={{
					top:
						activeIndex === null
							? 0
							: Math.max(
								64,
								Math.min(
									(rootRef.current?.clientHeight ?? 128) - 64,
									(viewportRef.current?.offsetTop ?? 0) +
										activeIndex * TICK_SPACING_PX -
										(viewportRef.current?.scrollTop ?? 0),
								),
							),
					visibility: activeItem ? "visible" : "hidden",
				}}
			>
				<p className="dcc-conversation-trail__prompt">
					{activeItem?.promptPreview}
				</p>
				{activeItem?.responsePreview ? (
					<p className="dcc-conversation-trail__response">
						{activeItem.responsePreview}
					</p>
				) : null}
			</div>
		</nav>
	);
}
