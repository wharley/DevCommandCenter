/**
 * The team room while its screen is open (`@dcc/mobile-core` teamRoom):
 * read on open, after the stream's lifecycle events (a turn started or
 * ended), on reconnect, when the app comes back to the front and right
 * after a message goes — always sending back the `snapshotId` held, so an
 * unchanged answer is a few bytes over the relay and no event is read.
 */
import { parseRoom, parseRoomSent, type RoomLine, type RoomSent } from "@dcc/mobile-core";
import { useCallback, useEffect, useRef, useState } from "react";
import { AppState } from "react-native";
import { ApiError, errorMessage, type Backend } from "./types";

/** Lifecycle events come in bursts: one read after them. */
const SETTLE_MS = 600;

export type TeamRoomState = {
	lines: RoomLine[];
	loading: boolean;
	/** The message goes to whoever the desktop's rule picks. */
	send: (text: string) => Promise<RoomSent>;
};

/** What the person reads when a message could not go. */
export function sendError(error: unknown): string {
	if (error instanceof ApiError && JSON.stringify(error.body ?? "").includes("roomNoConversation")) {
		return "Esse agente ainda não tem conversa: comece por ele na Sala do Mac.";
	}
	if (error instanceof ApiError && JSON.stringify(error.body ?? "").includes("roomNoAgent")) {
		return "Nenhum agente pode responder agora.";
	}
	return errorMessage(error, "Falha ao enviar.");
}

export function useTeamRoom(backend: Backend): TeamRoomState {
	const [lines, setLines] = useState<RoomLine[]>([]);
	const [loading, setLoading] = useState(true);
	const snapshot = useRef<string | null>(null);
	const reading = useRef(false);
	const settle = useRef<ReturnType<typeof setTimeout> | null>(null);

	const refresh = useCallback(() => {
		if (reading.current) return;
		reading.current = true;
		const after = snapshot.current;
		const query = after ? `?after=${encodeURIComponent(after)}` : "";
		backend
			.request("GET", `/api/v1/mobile/room${query}`)
			.then((data) => {
				const read = parseRoom(data, after);
				if (!read) {
					snapshot.current = null;
					return;
				}
				snapshot.current = read.snapshotId;
				if (!read.unchanged) setLines(read.lines);
			})
			// Older desktops have no route: an empty room.
			.catch(() => {})
			.finally(() => {
				reading.current = false;
				setLoading(false);
			});
	}, [backend]);

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
		const appState = AppState.addEventListener("change", (state) => {
			if (state === "active") refresh();
		});
		return () => {
			stop();
			if (settle.current) clearTimeout(settle.current);
			appState.remove();
		};
	}, [backend, refresh]);

	const send = useCallback(
		async (text: string) => {
			const data = await backend.request("POST", "/api/v1/mobile/room/messages", { text });
			const sent = parseRoomSent(data);
			if (!sent) throw new Error("Resposta inesperada do computador.");
			refresh();
			return sent;
		},
		[backend, refresh],
	);

	return { lines, loading, send };
}
