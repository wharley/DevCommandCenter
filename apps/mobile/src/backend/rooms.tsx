/**
 * The rooms for the home (`@dcc/mobile-core` rooms): read on open, after the
 * stream's lifecycle events (a turn started or ended), on reconnect, when
 * the app comes back to the front and once a minute while it is up — always
 * sending back the `snapshotId` held, so an unchanged answer is a few bytes
 * over the relay. What is unread comes from this phone (`roomsSeen`).
 */
import { parseRooms, sortRooms, type Room } from "@dcc/mobile-core";
import { useCallback, useEffect, useRef, useState } from "react";
import { AppState } from "react-native";
import { loadRoomsSeen, onRoomsSeen, type StoredRoomsSeen } from "./roomsSeen";
import { ApiError, type Backend } from "./types";

const POLL_MS = 60_000;
/** Lifecycle events come in bursts: one read after them. */
const SETTLE_MS = 600;

export type RoomsState = {
	/** Null until the first answer. */
	rooms: Room[] | null;
	/** The desktop is older than the rooms: the phone shows the one team room. */
	unsupported: boolean;
	seen: StoredRoomsSeen | null;
	refresh: () => void;
};

export function useRooms(backend: Backend): RoomsState {
	const [rooms, setRooms] = useState<Room[] | null>(null);
	const [unsupported, setUnsupported] = useState(false);
	const [seen, setSeen] = useState<StoredRoomsSeen | null>(null);
	const snapshot = useRef<string | null>(null);
	const reading = useRef(false);
	const settle = useRef<ReturnType<typeof setTimeout> | null>(null);

	const refresh = useCallback(() => {
		if (reading.current) return;
		reading.current = true;
		const after = snapshot.current;
		const query = after ? `?after=${encodeURIComponent(after)}` : "";
		backend
			.request("GET", `/api/v1/mobile/rooms${query}`)
			.then((data) => {
				const read = parseRooms(data, after);
				if (!read) {
					snapshot.current = null;
					return;
				}
				snapshot.current = read.snapshotId;
				setUnsupported(false);
				if (!read.unchanged) setRooms(sortRooms(read.rooms));
			})
			.catch((error: unknown) => {
				// An older desktop has no route; any other failure keeps what is shown.
				if (error instanceof ApiError && error.status === 404) {
					setUnsupported(true);
					setRooms((current) => current ?? []);
				}
			})
			.finally(() => {
				reading.current = false;
			});
	}, [backend]);

	useEffect(() => {
		void loadRoomsSeen().then(setSeen);
		return onRoomsSeen(setSeen);
	}, []);

	useEffect(() => {
		refresh();
		const later = () => {
			if (settle.current) clearTimeout(settle.current);
			settle.current = setTimeout(refresh, SETTLE_MS);
		};
		const stop = backend.subscribe(() => "/api/v1/events/stream?kinds=lifecycle", {
			onOpen: (reopened) => {
				if (reopened) refresh();
			},
			onEvent: () => later(),
		});
		const poll = setInterval(() => {
			if (AppState.currentState === "active") refresh();
		}, POLL_MS);
		const appState = AppState.addEventListener("change", (state) => {
			if (state === "active") refresh();
		});
		return () => {
			stop();
			clearInterval(poll);
			if (settle.current) clearTimeout(settle.current);
			appState.remove();
		};
	}, [backend, refresh]);

	return { rooms, unsupported, seen, refresh };
}
