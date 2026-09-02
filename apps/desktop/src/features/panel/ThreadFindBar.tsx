import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, ChevronUp, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { findInThread, stepThreadFindIndex } from "./thread-find.logic";
import type { WorkspaceMessage } from "./thread-projection";

/**
 * Find in the current conversation. Matches are computed from the projected
 * messages; navigation asks the viewport to reveal and focus one message.
 */
export function ThreadFindBar({
	messages,
	onFocusMessage,
	onClose,
}: {
	messages: WorkspaceMessage[];
	onFocusMessage: (messageId: string) => void;
	onClose: () => void;
}) {
	const { t } = useTranslation("common");
	const [query, setQuery] = useState("");
	const [activeIndex, setActiveIndex] = useState(0);
	const inputRef = useRef<HTMLInputElement | null>(null);
	const matches = useMemo(() => findInThread(messages, query), [messages, query]);
	const active = matches[Math.min(activeIndex, Math.max(0, matches.length - 1))] ?? null;

	useEffect(() => {
		inputRef.current?.focus();
	}, []);
	// Keep the active index valid when the match list changes underneath.
	useEffect(() => {
		setActiveIndex((current) => (matches.length === 0 ? 0 : Math.min(current, matches.length - 1)));
	}, [matches.length]);
	useEffect(() => {
		if (active) onFocusMessage(active.messageId);
		// Only the focused match matters here; the callback identity is stable per panel.
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [active?.messageId]);

	const step = (direction: 1 | -1) =>
		setActiveIndex((current) => stepThreadFindIndex(current, matches.length, direction));

	return (
		<div
			className="flex shrink-0 items-center gap-2 border-b border-border/60 bg-background/95 px-4 py-1.5 backdrop-blur"
			role="search"
			data-testid="thread-find-bar"
		>
			<Input
				ref={inputRef}
				value={query}
				onChange={(event) => {
					setQuery(event.target.value);
					setActiveIndex(0);
				}}
				onKeyDown={(event) => {
					if (event.key === "Escape") {
						event.preventDefault();
						onClose();
					} else if (event.key === "Enter") {
						event.preventDefault();
						step(event.shiftKey ? -1 : 1);
					}
				}}
				placeholder={t("conversation.find.placeholder")}
				aria-label={t("conversation.find.label")}
				className="h-7 max-w-xs text-[12px]"
			/>
			<span className="min-w-0 flex-1 truncate text-[11px] tabular-nums text-muted-foreground">
				{query.trim().length === 0
					? t("conversation.find.hint")
					: matches.length === 0
						? t("conversation.find.empty")
						: t("conversation.find.count", { current: activeIndex + 1, total: matches.length })}
				{active ? ` · ${active.snippet}` : ""}
			</span>
			<Button
				type="button"
				variant="ghost"
				size="icon-sm"
				aria-label={t("conversation.find.previous")}
				disabled={matches.length === 0}
				onClick={() => step(-1)}
			>
				<ChevronUp className="size-3.5" />
			</Button>
			<Button
				type="button"
				variant="ghost"
				size="icon-sm"
				aria-label={t("conversation.find.next")}
				disabled={matches.length === 0}
				onClick={() => step(1)}
			>
				<ChevronDown className="size-3.5" />
			</Button>
			<Button
				type="button"
				variant="ghost"
				size="icon-sm"
				aria-label={t("conversation.find.close")}
				onClick={onClose}
			>
				<X className="size-3.5" />
			</Button>
		</div>
	);
}
