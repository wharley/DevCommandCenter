import { useEffect, useState, useSyncExternalStore } from "react";

/**
 * The chronicler's recap window. A new recap starts at the first look after
 * 4am local time and covers everything since the previous recap ended, so
 * Monday's recap covers the weekend and a second opening the same day does
 * not produce a recap of a few hours. Kept in this browser profile: it is a
 * reading marker, not task state.
 */
const STORAGE_KEY = "dcc.agents.dailyRecap";
export const RECAP_HOUR = 4;
const MAX_WINDOW_DAYS = 7;
const DAY_MS = 24 * 60 * 60 * 1000;
const CLOCK_TICK_MS = 5 * 60 * 1000;

export type RecapWindow = { from: string; to: string };
export type DailyRecapState = {
	current: RecapWindow | null;
	/** `to` of the window the person last looked at. */
	seenTo: string | null;
};

const EMPTY_STATE: DailyRecapState = { current: null, seenTo: null };

/** The latest 4am local time at or before `now`. */
export function recapBoundary(now: Date): Date {
	const boundary = new Date(now);
	boundary.setHours(RECAP_HOUR, 0, 0, 0);
	if (boundary.getTime() > now.getTime()) {
		boundary.setDate(boundary.getDate() - 1);
	}
	return boundary;
}

/**
 * Opens a new window once the current one ended before today's 4am. The new
 * one starts where the previous ended (a day before the boundary the first
 * time), never more than a week back, and ends now.
 */
export function nextRecapState(state: DailyRecapState, now: Date): DailyRecapState {
	const boundary = recapBoundary(now);
	const currentTo = state.current ? Date.parse(state.current.to) : Number.NaN;
	if (!Number.isNaN(currentTo) && currentTo >= boundary.getTime()) {
		return state;
	}
	const previousEnd = Number.isNaN(currentTo) ? boundary.getTime() - DAY_MS : currentTo;
	const from = Math.max(previousEnd, now.getTime() - MAX_WINDOW_DAYS * DAY_MS);
	return {
		...state,
		current: { from: new Date(from).toISOString(), to: now.toISOString() },
	};
}

export function isRecapUnread(state: DailyRecapState): boolean {
	return Boolean(state.current) && state.seenTo !== state.current?.to;
}

const listeners = new Set<() => void>();
let cache: DailyRecapState | null = null;

function isWindow(value: unknown): value is RecapWindow {
	return (
		Boolean(value) &&
		typeof (value as RecapWindow).from === "string" &&
		typeof (value as RecapWindow).to === "string"
	);
}

function read(): DailyRecapState {
	if (cache) {
		return cache;
	}
	try {
		const parsed = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "null") as Partial<
			DailyRecapState
		> | null;
		cache = {
			current: isWindow(parsed?.current) ? parsed.current : null,
			seenTo: typeof parsed?.seenTo === "string" ? parsed.seenTo : null,
		};
	} catch {
		cache = EMPTY_STATE;
	}
	return cache;
}

function write(next: DailyRecapState): void {
	cache = next;
	try {
		window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
	} catch {
		// Storage can be unavailable or full; the window then lasts for this run.
	}
	for (const listener of listeners) {
		listener();
	}
}

/** Opens the next window when its time has come; a no-op otherwise. */
export function refreshDailyRecap(now: Date = new Date()): void {
	const state = read();
	const next = nextRecapState(state, now);
	if (next !== state) {
		write(next);
	}
}

export function markDailyRecapSeen(): void {
	const state = read();
	if (state.current && state.seenTo !== state.current.to) {
		write({ ...state, seenTo: state.current.to });
	}
}

/**
 * The current recap window, opening a new one at the first look after 4am,
 * including when the app stays open overnight.
 */
export function useDailyRecap(): DailyRecapState {
	const [now, setNow] = useState(() => new Date());
	useEffect(() => {
		const timer = window.setInterval(() => setNow(new Date()), CLOCK_TICK_MS);
		return () => window.clearInterval(timer);
	}, []);
	useEffect(() => {
		refreshDailyRecap(now);
	}, [now]);
	return useSyncExternalStore(
		(listener) => {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		read,
		read,
	);
}
