/**
 * A room while its screen is open (`@dcc/mobile-core` teamRoom) — Geral, or
 * a project's with `room`; its approvals and questions are answered here too:
 * read on open, after the stream's lifecycle events (a turn started or
 * ended), on reconnect, when the app comes back to the front and right
 * after a message goes — always sending back the `snapshotId` held, so an
 * unchanged answer is a few bytes over the relay and no event is read.
 */
import { parseRoom, parseRoomSent, type RoomApproval, type RoomLine, type RoomQuestion, type RoomSent } from "@dcc/mobile-core";
import { useCallback, useEffect, useRef, useState } from "react";
import { AppState } from "react-native";
import { ApiError, errorMessage, type Backend } from "./types";

/** Lifecycle events come in bursts: one read after them. */
const SETTLE_MS = 600;

export type TeamRoomState = {
	lines: RoomLine[];
	/** The permission requests waiting in the room. */
	approvals: RoomApproval[];
	/** The agents' questions waiting in the room, oldest first. */
	questions: RoomQuestion[];
	/** The agents mid-turn in the room; null when the desktop does not say. */
	working: string[] | null;
	/** The approvals and questions being answered now, by request. */
	answering: ReadonlySet<string>;
	loading: boolean;
	/** Allows or denies one approval; resolves to an error text, or null. */
	answer: (approval: RoomApproval, allow: boolean) => Promise<string | null>;
	/** Sends a question's answers; resolves to an error text, or null. */
	reply: (question: RoomQuestion, answers: Array<{ question: string; answer: string }>) => Promise<string | null>;
	/** The message goes to whoever the desktop's rule picks. */
	send: (text: string) => Promise<RoomSent>;
};

/** What the person reads when a message could not go. */
export function sendError(error: unknown): string {
	if (error instanceof ApiError && JSON.stringify(error.body ?? "").includes("roomNoConversation")) {
		return "Esse agente ainda não tem conversa nesta sala: comece por ele no Mac, ou atualize o DCC de lá.";
	}
	if (error instanceof ApiError && JSON.stringify(error.body ?? "").includes("agentLimitReached")) {
		return "Esse agente chegou ao limite do mês. Libere no Mac, na página dele, para continuar.";
	}
	if (error instanceof ApiError && JSON.stringify(error.body ?? "").includes("roomNoAgent")) {
		return "Nenhum agente pode responder agora.";
	}
	return errorMessage(error, "Falha ao enviar.");
}

export function useTeamRoom(backend: Backend, roomId: string | null = null): TeamRoomState {
	const [lines, setLines] = useState<RoomLine[]>([]);
	const [approvals, setApprovals] = useState<RoomApproval[]>([]);
	const [questions, setQuestions] = useState<RoomQuestion[]>([]);
	const [working, setWorking] = useState<string[] | null>(null);
	const [answering, setAnswering] = useState<ReadonlySet<string>>(() => new Set());
	const [loading, setLoading] = useState(true);
	const snapshot = useRef<string | null>(null);
	const reading = useRef(false);
	const settle = useRef<ReturnType<typeof setTimeout> | null>(null);

	const refresh = useCallback(() => {
		if (reading.current) return;
		reading.current = true;
		const after = snapshot.current;
		const params = [roomId ? `room=${encodeURIComponent(roomId)}` : null, after ? `after=${encodeURIComponent(after)}` : null].filter(Boolean);
		const query = params.length > 0 ? `?${params.join("&")}` : "";
		backend
			.request("GET", `/api/v1/mobile/room${query}`)
			.then((data) => {
				const read = parseRoom(data, after);
				if (!read) {
					snapshot.current = null;
					return;
				}
				snapshot.current = read.snapshotId;
				if (!read.unchanged) {
					setLines(read.lines);
					setApprovals(read.approvals);
					setQuestions(read.questions);
					setWorking(read.working);
				}
			})
			// Older desktops have no route: an empty room.
			.catch(() => {})
			.finally(() => {
				reading.current = false;
				setLoading(false);
			});
	}, [backend, roomId]);

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
			const data = await backend.request("POST", "/api/v1/mobile/room/messages", roomId ? { text, room: roomId } : { text });
			const sent = parseRoomSent(data);
			if (!sent) throw new Error("Resposta inesperada do computador.");
			refresh();
			return sent;
		},
		[backend, refresh, roomId],
	);

	const answer = useCallback(
		async (approval: RoomApproval, allow: boolean) => {
			setAnswering((current) => new Set(current).add(approval.requestId));
			try {
				await backend.request("POST", `/api/v1/sessions/${encodeURIComponent(approval.sessionId)}/respond-permission`, {
					sessionId: approval.sessionId,
					requestId: approval.requestId,
					behavior: allow ? "allow" : "deny",
				});
				// Gone from the room at once; the next read confirms it.
				setApprovals((current) => current.filter((item) => item.requestId !== approval.requestId));
				refresh();
				return null;
			} catch (error) {
				refresh();
				return errorMessage(error, "Não foi possível responder.");
			} finally {
				setAnswering((current) => {
					const next = new Set(current);
					next.delete(approval.requestId);
					return next;
				});
			}
		},
		[backend, refresh],
	);

	/** Marks a request as being answered while `run` goes. */
	const busy = useCallback(async (requestId: string, run: () => Promise<string | null>) => {
		setAnswering((current) => new Set(current).add(requestId));
		try {
			return await run();
		} finally {
			setAnswering((current) => {
				const next = new Set(current);
				next.delete(requestId);
				return next;
			});
		}
	}, []);

	const reply = useCallback(
		(question: RoomQuestion, answers: Array<{ question: string; answer: string }>) =>
			busy(question.requestId, async () => {
				try {
					await backend.request("POST", `/api/v1/sessions/${encodeURIComponent(question.sessionId)}/respond-user-input`, {
						sessionId: question.sessionId,
						requestId: question.requestId,
						answers,
					});
					// Gone from the room at once; the next read confirms it.
					setQuestions((current) => current.filter((item) => item.requestId !== question.requestId));
					refresh();
					return null;
				} catch (error) {
					refresh();
					if (error instanceof ApiError && error.status === 409) return "Essa pergunta já tinha sido respondida em outro lugar.";
					return errorMessage(error, "Não foi possível responder.");
				}
			}),
		[backend, busy, refresh],
	);

	return { lines, approvals, questions, working, answering, loading, send, answer, reply };
}
