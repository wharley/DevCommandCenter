/**
 * A room while its screen is open (`@dcc/mobile-core` teamRoom) — Geral, or
 * a project's with `room`; its approvals and questions are answered here too:
 * read on open, after the stream's lifecycle events (a turn started or
 * ended), on reconnect, when the app comes back to the front and right
 * after a message goes — always sending back the `snapshotId` held, so an
 * unchanged answer is a few bytes over the relay and no event is read.
 *
 * While an agent works in the room (`live`) it is also read every
 * `LIVE_POLL_MS`, only while the screen is in front and the app active,
 * sending back the `linesId` held so only the turns at work come; nothing
 * is polled otherwise. Each read keeps the objects that did not change
 * (`keepUnchanged`), so the list redraws only the rows that moved.
 */
import {
	keepUnchanged,
	parseRoom,
	parseRoomSent,
	parseRoomSteps,
	type RoomApproval,
	type RoomCard,
	type RoomLine,
	type RoomLive,
	type RoomQueued,
	type RoomQuestion,
	type RoomReaction,
	type RoomSent,
	type RoomStep,
	type RoomStopped,
} from "@dcc/mobile-core";
import { useCallback, useEffect, useRef, useState } from "react";
import { AppState } from "react-native";
import { ApiError, errorMessage, type Backend } from "./types";

/** Lifecycle events come in bursts: one read after them. */
const SETTLE_MS = 600;
/** How often a room with an agent at work is read. */
const LIVE_POLL_MS = 1500;

export type ApprovalChoice = "allow" | "always" | "deny";

export type TeamRoomState = {
	lines: RoomLine[];
	/** The permission requests waiting in the room. */
	approvals: RoomApproval[];
	/** The agents' questions waiting in the room, oldest first. */
	questions: RoomQuestion[];
	/** The agents mid-turn in the room; null when the desktop does not say. */
	working: string[] | null;
	/** Each turn at work in the room, and the tasks its agents opened. */
	live: RoomLive[];
	/** The turns that stopped in the last day, to try again. */
	stopped: RoomStopped[];
	/** The messages waiting for a turn to end. */
	queued: RoomQueued[];
	/** The room's cards: results, check-ins, and DCC's own (limits, proposals). */
	cards: RoomCard[];
	/** The approvals, questions and other actions running now, by key. */
	answering: ReadonlySet<string>;
	loading: boolean;
	/** Answers one approval (a denial may carry a reason); resolves to an error text, or null. */
	answer: (approval: RoomApproval, choice: ApprovalChoice, reason?: string) => Promise<string | null>;
	/** Sends a question's answers; resolves to an error text, or null. */
	reply: (question: RoomQuestion, answers: Array<{ question: string; answer: string }>) => Promise<string | null>;
	/** The message goes to whoever the desktop's rule picks. */
	send: (text: string) => Promise<RoomSent>;
	/** "Tentar de novo": the stopped turn's prompt to its conversation again. */
	retry: (stopped: RoomStopped) => Promise<string | null>;
	/** "Parar": stops a turn at work. */
	stop: (live: RoomLive) => Promise<string | null>;
	/** "Passar a vez" or "Ignorar" under an answer that suggests a teammate. */
	handoff: (line: RoomLine, action: "pass" | "ignore") => Promise<string | null>;
	/** An answer's steps, read when the person opens them. */
	steps: (line: RoomLine) => Promise<{ steps: RoomStep[]; more: number }>;
	/** 👍 / 👎 on an answer; null takes it back. */
	react: (line: RoomLine, reaction: RoomReaction | null) => Promise<string | null>;
	/** "Guardar como regra?": the person's note in the agent's memory. */
	keepRule: (agentId: string, body: string) => Promise<string | null>;
	/** "Criar" / "Descartar" on a proposed agent, routine or source. */
	answerProposal: (card: RoomCard, action: "create" | "discard") => Promise<string | null>;
	/** "Continuar mesmo assim" on an agent past its monthly limit. */
	continueLimit: (card: RoomCard) => Promise<string | null>;
	/** "Enviar no Slack" / "Descartar" on a reply an agent proposed: the desktop sends it. */
	answerReply: (card: RoomCard, action: "send" | "discard") => Promise<string | null>;
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
	if (error instanceof ApiError && JSON.stringify(error.body ?? "").includes("roomNoHandoff")) {
		return "Essa sugestão já não está na sala.";
	}
	return errorMessage(error, "Falha ao enviar.");
}

function sameIds(left: string[] | null, right: string[] | null): boolean {
	return left === right || (!!left && !!right && left.length === right.length && left.every((id, i) => id === right[i]));
}

/** The room of `roomId`; `focused` while its screen is in front (it polls only then). */
export function useTeamRoom(backend: Backend, roomId: string | null = null, focused = true): TeamRoomState {
	const [lines, setLines] = useState<RoomLine[]>([]);
	const [approvals, setApprovals] = useState<RoomApproval[]>([]);
	const [questions, setQuestions] = useState<RoomQuestion[]>([]);
	const [working, setWorking] = useState<string[] | null>(null);
	const [live, setLive] = useState<RoomLive[]>([]);
	const [stopped, setStopped] = useState<RoomStopped[]>([]);
	const [queued, setQueued] = useState<RoomQueued[]>([]);
	const [cards, setCards] = useState<RoomCard[]>([]);
	const [answering, setAnswering] = useState<ReadonlySet<string>>(() => new Set());
	const [loading, setLoading] = useState(true);
	const snapshot = useRef<string | null>(null);
	const linesId = useRef<string | null>(null);
	const reading = useRef(false);
	const settle = useRef<ReturnType<typeof setTimeout> | null>(null);

	const refresh = useCallback(() => {
		if (reading.current) return;
		reading.current = true;
		const after = snapshot.current;
		const held = linesId.current;
		const params = [
			roomId ? `room=${encodeURIComponent(roomId)}` : null,
			after ? `after=${encodeURIComponent(after)}` : null,
			held ? `lines=${encodeURIComponent(held)}` : null,
		].filter(Boolean);
		const query = params.length > 0 ? `?${params.join("&")}` : "";
		backend
			.request("GET", `/api/v1/mobile/room${query}`)
			.then((data) => {
				const read = parseRoom(data, after);
				if (!read) {
					snapshot.current = null;
					linesId.current = null;
					return;
				}
				snapshot.current = read.snapshotId;
				if (read.unchanged) return;
				if (!read.linesUnchanged) {
					setLines((current) => keepUnchanged(current, read.lines, (line) => line.key));
				}
				linesId.current = read.linesId;
				setApprovals((current) => keepUnchanged(current, read.approvals, (item) => item.requestId));
				setQuestions((current) => keepUnchanged(current, read.questions, (item) => item.requestId));
				setWorking((current) => (sameIds(current, read.working) ? current : read.working));
				setLive((current) => keepUnchanged(current, read.live, (item) => item.key));
				setStopped((current) => keepUnchanged(current, read.stopped, (item) => `${item.sessionId}:${item.turnId}`));
				setQueued((current) => keepUnchanged(current, read.queued, (item) => item.id));
				setCards((current) => keepUnchanged(current, read.cards, (item) => item.key));
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

	// Someone at work: read on a beat while the room is on screen.
	const atWork = live.length > 0;
	useEffect(() => {
		if (!atWork || !focused) return;
		const beat = setInterval(() => {
			if (AppState.currentState === "active") refresh();
		}, LIVE_POLL_MS);
		return () => clearInterval(beat);
	}, [atWork, focused, refresh]);

	/** Marks `key` as running while `run` goes. */
	const busy = useCallback(async (key: string, run: () => Promise<string | null>) => {
		setAnswering((current) => new Set(current).add(key));
		try {
			return await run();
		} finally {
			setAnswering((current) => {
				const next = new Set(current);
				next.delete(key);
				return next;
			});
		}
	}, []);

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
		(approval: RoomApproval, choice: ApprovalChoice, reason?: string) =>
			busy(approval.requestId, async () => {
				try {
					const why = reason?.trim();
					if (choice === "deny" && why) {
						// The desktop sends the reason as the next message where the provider cannot read it.
						await backend.request("POST", "/api/v1/mobile/room/approvals/respond", {
							sessionId: approval.sessionId,
							requestId: approval.requestId,
							behavior: "deny",
							message: why,
							toolName: approval.toolName,
						});
					} else {
						await backend.request("POST", `/api/v1/sessions/${encodeURIComponent(approval.sessionId)}/respond-permission`, {
							sessionId: approval.sessionId,
							requestId: approval.requestId,
							behavior: choice === "deny" ? "deny" : "allow",
							always: choice === "always",
						});
					}
					// Gone from the room at once; the next read confirms it.
					setApprovals((current) => current.filter((item) => item.requestId !== approval.requestId));
					refresh();
					return null;
				} catch (error) {
					refresh();
					return errorMessage(error, "Não foi possível responder.");
				}
			}),
		[backend, busy, refresh],
	);

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

	const retry = useCallback(
		(turn: RoomStopped) =>
			busy(`retry:${turn.turnId}`, async () => {
				if (!turn.retry) return null;
				try {
					await backend.request("POST", "/api/v1/mobile/room/messages", {
						text: turn.retry.prompt,
						sessionId: turn.retry.sessionId,
						retryOf: turn.retry.turnId,
						...(roomId ? { room: roomId } : {}),
					});
					setStopped((current) => current.filter((item) => item !== turn));
					refresh();
					return null;
				} catch (error) {
					return `Não deu para tentar de novo: ${sendError(error)}`;
				}
			}),
		[backend, busy, refresh, roomId],
	);

	const stop = useCallback(
		(turn: RoomLive) =>
			busy(`stop:${turn.sessionId}`, async () => {
				try {
					await backend.request("POST", `/api/v1/sessions/${encodeURIComponent(turn.sessionId)}/abort`, { sessionId: turn.sessionId });
					refresh();
					return null;
				} catch (error) {
					return `Não deu para parar: ${errorMessage(error, "falhou")}`;
				}
			}),
		[backend, busy, refresh],
	);

	const handoff = useCallback(
		(line: RoomLine, action: "pass" | "ignore") =>
			busy(`handoff:${line.key}`, async () => {
				try {
					await backend.request("POST", "/api/v1/mobile/room/handoff", { key: line.key, action, ...(roomId ? { room: roomId } : {}) });
					refresh();
					return null;
				} catch (error) {
					return sendError(error);
				}
			}),
		[backend, busy, refresh, roomId],
	);

	const steps = useCallback(
		async (line: RoomLine) => {
			const turn = line.turnId ?? line.key.slice(line.sessionId.length + 1);
			const data = await backend.request(
				"GET",
				`/api/v1/mobile/room/steps?sessionId=${encodeURIComponent(line.sessionId)}&turn=${encodeURIComponent(turn)}`,
			);
			const read = parseRoomSteps(data);
			if (!read) throw new Error("Resposta inesperada do computador.");
			return read;
		},
		[backend],
	);

	const react = useCallback(
		(line: RoomLine, reaction: RoomReaction | null) =>
			busy(`react:${line.key}`, async () => {
				const before = line.reaction;
				// Shown at once; the next read confirms it.
				setLines((current) => current.map((item) => (item.key === line.key ? { ...item, reaction } : item)));
				try {
					await backend.request("POST", "/api/v1/mobile/room/reactions", { sessionId: line.sessionId, lineKey: line.key, reaction });
					refresh();
					return null;
				} catch (error) {
					setLines((current) => current.map((item) => (item.key === line.key ? { ...item, reaction: before } : item)));
					return errorMessage(error, "Não foi possível reagir.");
				}
			}),
		[backend, busy, refresh],
	);

	const keepRule = useCallback(
		(agentId: string, body: string) =>
			busy(`rule:${agentId}`, async () => {
				try {
					await backend.request("POST", "/api/v1/mobile/room/rules", { agentId, body });
					return null;
				} catch (error) {
					return errorMessage(error, "Não foi possível guardar a regra.");
				}
			}),
		[backend, busy],
	);

	const answerProposal = useCallback(
		(card: RoomCard, action: "create" | "discard") =>
			busy(`card:${card.key}`, async () => {
				const kind = card.kind === "agentProposal" ? "agent" : card.kind === "routineProposal" ? "routine" : "source";
				const id = card.proposal?.id ?? card.routine?.id ?? card.source?.id;
				if (!id) return null;
				try {
					await backend.request("POST", "/api/v1/mobile/room/proposals", { kind, id, agentId: card.agentId, action });
					// Gone from the room at once; the next read confirms it.
					setCards((current) => current.filter((item) => item.key !== card.key));
					refresh();
					return null;
				} catch (error) {
					refresh();
					return errorMessage(error, "Não foi possível responder à proposta.");
				}
			}),
		[backend, busy, refresh],
	);

	const answerReply = useCallback(
		(card: RoomCard, action: "send" | "discard") =>
			busy(`card:${card.key}`, async () => {
				if (!card.reply) return null;
				try {
					await backend.request("POST", "/api/v1/mobile/room/replies", { id: card.reply.id, action });
					setCards((current) => current.filter((item) => item.key !== card.key));
					refresh();
					return null;
				} catch (error) {
					// A failed send stays, with why, on the next read.
					refresh();
					return errorMessage(error, action === "send" ? "Não foi possível enviar a resposta." : "Não foi possível descartar.");
				}
			}),
		[backend, busy, refresh],
	);

	const continueLimit = useCallback(
		(card: RoomCard) =>
			busy(`card:${card.key}`, async () => {
				try {
					await backend.request("POST", "/api/v1/mobile/room/limit", { agentId: card.agentId });
					setCards((current) => current.filter((item) => item.key !== card.key));
					refresh();
					return null;
				} catch (error) {
					return errorMessage(error, "Não foi possível liberar o agente.");
				}
			}),
		[backend, busy, refresh],
	);

	return {
		lines,
		approvals,
		questions,
		working,
		live,
		stopped,
		queued,
		cards,
		answering,
		loading,
		send,
		answer,
		reply,
		retry,
		stop,
		handoff,
		steps,
		react,
		keepRule,
		answerProposal,
		continueLimit,
		answerReply,
	};
}
