import { createContext, useContext, useEffect, useState } from "react";

/** "Expand all" / "Collapse all" from the review header; `version` repeats it. */
export type ReviewCardsCommand = { open: boolean; version: number };

type ReviewCardsOpen = {
	defaultOpen: boolean;
	command: ReviewCardsCommand | null;
};

const ReviewCardsOpenContext = createContext<ReviewCardsOpen>({
	defaultOpen: true,
	command: null,
});

export const ReviewCardsOpenProvider = ReviewCardsOpenContext.Provider;

/**
 * Open state of one diff card: starts from the list's default (or the last
 * expand/collapse-all), follows later expand/collapse-all clicks, and the
 * card's own chevron toggles it in between.
 */
export function useReviewCardOpen(forceOpen = false) {
	const { defaultOpen, command } = useContext(ReviewCardsOpenContext);
	const [open, setOpen] = useState(() => forceOpen || (command?.open ?? defaultOpen));
	const commandOpen = command?.open;
	const commandVersion = command?.version;
	useEffect(() => {
		if (commandOpen !== undefined) setOpen(commandOpen);
	}, [commandOpen, commandVersion]);
	useEffect(() => {
		if (forceOpen) setOpen(true);
	}, [forceOpen]);
	return [open, setOpen] as const;
}
