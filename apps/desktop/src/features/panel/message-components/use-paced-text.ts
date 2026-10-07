import { useEffect, useRef, useState } from "react";

/** Seconds the reveal aims to need to catch up with what already arrived. */
const CATCH_UP_SECONDS = 0.25;
const MIN_CHARS_PER_SECOND = 90;
const MAX_CHARS_PER_SECOND = 2400;
/** React commits at most this often; the frame loop only advances a ref. */
const COMMIT_INTERVAL_MS = 40;
/** A half-written word is held back at most this many characters. */
const WORD_LOOKAHEAD = 12;
const INITIAL_REPLAY_CHARS = 240;

/**
 * Next reveal length for one frame. Pure so the pacing can be tested without
 * timers: the speed adapts to the backlog, so a burst of tokens is spread over
 * ~250ms instead of landing in a single paint, and a slow stream never stalls.
 */
export function nextPacedLength(
	shown: number,
	target: string,
	elapsedMs: number,
): number {
	const backlog = target.length - shown;
	if (backlog <= 0) return target.length;
	const rate = Math.min(
		MAX_CHARS_PER_SECOND,
		Math.max(MIN_CHARS_PER_SECOND, backlog / CATCH_UP_SECONDS),
	);
	let next = Math.min(target.length, shown + Math.max(1, Math.round((rate * elapsedMs) / 1000)));
	if (next < target.length) {
		const boundary = target.slice(next, next + WORD_LOOKAHEAD).search(/\s/);
		if (boundary >= 0) next += boundary;
	}
	return next;
}

/**
 * Reveals streamed text at an even pace. Inactive (settled) text is returned
 * as-is; a non-append change (the provider replaced its snapshot) snaps.
 */
export function usePacedText(target: string, active: boolean): string {
	// Text that already existed when the row mounted (switching back to a
	// running session) is not replayed from the start.
	const [shownLength, setShownLength] = useState(() =>
		active ? Math.max(0, target.length - INITIAL_REPLAY_CHARS) : target.length,
	);
	const shownRef = useRef(shownLength);
	const targetRef = useRef(target);
	const previousTargetRef = useRef(target);

	if (previousTargetRef.current !== target) {
		const appended = target.startsWith(previousTargetRef.current.slice(0, shownRef.current));
		if (!appended) shownRef.current = Math.min(shownRef.current, target.length);
		previousTargetRef.current = target;
	}
	targetRef.current = target;

	useEffect(() => {
		if (!active) return;
		let frame = 0;
		let last = performance.now();
		let lastCommit = 0;
		const tick = (now: number) => {
			const elapsed = now - last;
			last = now;
			const current = targetRef.current;
			if (shownRef.current < current.length) {
				shownRef.current = nextPacedLength(shownRef.current, current, elapsed);
				if (now - lastCommit >= COMMIT_INTERVAL_MS || shownRef.current >= current.length) {
					lastCommit = now;
					setShownLength(shownRef.current);
				}
			}
			frame = requestAnimationFrame(tick);
		};
		frame = requestAnimationFrame(tick);
		return () => cancelAnimationFrame(frame);
	}, [active]);

	if (!active) return target;
	return target.slice(0, Math.min(shownLength, target.length));
}
