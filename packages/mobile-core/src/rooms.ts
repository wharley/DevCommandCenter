/**
 * The rooms as the desktop serves them to the phone
 * (`GET /api/v1/mobile/rooms`): Geral (`id: null`) and one per project, each
 * with its latest line, who works there and whether something waits on the
 * person. The phone's home lists them like a messages app. A phone that
 * sends back the `snapshotId` it holds gets `{ unchanged, snapshotId }`.
 *
 * What is unread is the phone's own: the latest answer of a room against
 * when this phone last opened it.
 */

export type RoomLast = {
	agentId: string;
	agentName: string;
	speaker: "person" | "agent";
	text: string;
	at: string;
};

export type Room = {
	/** The project's id; null is the Geral room. */
	id: string | null;
	name: string;
	last: RoomLast | null;
	/** The agents mid-turn in the room. */
	working: string[];
	/** A conversation of the room waits on the person. */
	needsYou: boolean;
	/** A task of the room's project runs. */
	running: boolean;
	at: string | null;
};

export type RoomsSnapshot =
	| { unchanged: true; snapshotId: string }
	| { unchanged: false; snapshotId: string | null; rooms: Room[] };

function text(value: unknown): string | null {
	return typeof value === "string" && value.length > 0 ? value : null;
}

function parseLast(raw: unknown): RoomLast | null {
	if (!raw || typeof raw !== "object") return null;
	const value = raw as Record<string, unknown>;
	const body = text(value.text);
	const at = text(value.at);
	if (!body || !at) return null;
	return {
		agentId: text(value.agentId) ?? "",
		agentName: text(value.agentName) ?? "",
		speaker: value.speaker === "agent" ? "agent" : "person",
		text: body,
		at,
	};
}

function parseRoomRow(raw: unknown): Room | null {
	if (!raw || typeof raw !== "object") return null;
	const value = raw as Record<string, unknown>;
	const name = text(value.name);
	if (!name) return null;
	const last = parseLast(value.last);
	return {
		id: text(value.id),
		name,
		last,
		working: Array.isArray(value.working) ? value.working.filter((id): id is string => typeof id === "string") : [],
		needsYou: value.needsYou === true,
		running: value.running === true,
		at: text(value.at) ?? last?.at ?? null,
	};
}

/** The route's answer; `unchanged` only counts when it answers `after`. */
export function parseRooms(data: unknown, after: string | null): RoomsSnapshot | null {
	if (!data || typeof data !== "object") return null;
	const value = data as Record<string, unknown>;
	const snapshotId = text(value.snapshotId);
	if (value.unchanged === true) {
		return snapshotId && snapshotId === after ? { unchanged: true, snapshotId } : null;
	}
	if (!Array.isArray(value.rooms)) return null;
	return {
		unchanged: false,
		snapshotId,
		rooms: value.rooms.flatMap((raw) => {
			const room = parseRoomRow(raw);
			return room ? [room] : [];
		}),
	};
}

/** The room's key on this phone (storage, list keys, the route's param). */
export function roomKey(id: string | null): string {
	return id ?? "geral";
}

/** When each room was last opened on this phone, by `roomKey`. */
export type RoomsSeen = Record<string, string>;

/**
 * News in a room: its latest line is an agent's and came after the phone
 * last opened the room. A room never opened counts from `baseline` (when
 * this phone started tracking), so old answers are not news.
 */
export function roomUnread(room: Room, seen: RoomsSeen, baseline: string | null): boolean {
	if (!room.last || room.last.speaker !== "agent") return false;
	const since = seen[roomKey(room.id)] ?? baseline;
	return since === null || since === undefined ? false : room.last.at > since;
}

/** What the row says under the room's name, and how loud. */
export function roomStatus(room: Room, nameOf: (agentId: string) => string | null): { tone: "wait" | "live" | "idle"; text: string } {
	if (room.needsYou) return { tone: "wait", text: "Precisa de você" };
	const names = room.working.map(nameOf).filter((name): name is string => !!name);
	if (names.length === 1) return { tone: "live", text: `${names[0]} está trabalhando…` };
	if (names.length > 1) return { tone: "live", text: `${names.length} agentes trabalhando…` };
	if (room.last) {
		const who = room.last.speaker === "agent" ? room.last.agentName : "Você";
		return { tone: room.running ? "live" : "idle", text: who ? `${who}: ${room.last.text}` : room.last.text };
	}
	if (room.running) return { tone: "live", text: "Uma tarefa está rodando" };
	return { tone: "idle", text: "Ninguém falou ainda" };
}

/** "09:41" today, "ontem", the weekday within a week, else "12/11". */
export function roomTime(at: string | null, now: Date = new Date()): string {
	if (!at) return "";
	const when = new Date(at);
	if (Number.isNaN(when.getTime())) return "";
	const day = (date: Date) => new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
	const days = Math.round((day(now) - day(when)) / 86_400_000);
	const two = (value: number) => String(value).padStart(2, "0");
	if (days <= 0) return `${two(when.getHours())}:${two(when.getMinutes())}`;
	if (days === 1) return "ontem";
	if (days < 7) return ["dom", "seg", "ter", "qua", "qui", "sex", "sáb"][when.getDay()] ?? "";
	return `${two(when.getDate())}/${two(when.getMonth() + 1)}`;
}

/** Rooms that ask for the person first, then the ones with something live, the rest as served. */
export function sortRooms(rooms: Room[]): Room[] {
	const weight = (room: Room) => (room.needsYou ? 0 : 1);
	return rooms
		.map((room, index) => ({ room, index }))
		.sort((left, right) => {
			// Geral always leads.
			if ((left.room.id === null) !== (right.room.id === null)) return left.room.id === null ? -1 : 1;
			return weight(left.room) - weight(right.room) || left.index - right.index;
		})
		.map(({ room }) => room);
}
