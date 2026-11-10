import { describe, expect, it } from "vitest";
import { parseRooms, roomKey, roomStatus, roomTime, roomUnread, sortRooms, type Room } from "./rooms.ts";

const room = (extra: Partial<Room> = {}): Room => ({ id: "p", name: "Checkout", last: null, working: [], needsYou: false, running: false, at: null, ...extra });

describe("rooms", () => {
	it("reads the desktop's rooms and only trusts unchanged for the id it sent", () => {
		const read = parseRooms(
			{
				snapshotId: "a1",
				rooms: [
					{ id: null, name: "Geral", last: null, working: [], needsYou: false, at: null },
					{ id: "p", name: "Checkout", last: { agentId: "dev", agentName: "Dev", speaker: "agent", text: "Pronto.", at: "2026-11-10T12:00:00Z" }, working: ["dev", 3], needsYou: true, running: true, at: null },
					{ id: "x" },
				],
			},
			null,
		);
		expect(read && !read.unchanged && read.rooms.map((r) => r.name)).toEqual(["Geral", "Checkout"]);
		const checkout = read && !read.unchanged ? read.rooms[1]! : room();
		expect(checkout.working).toEqual(["dev"]);
		expect(checkout.at).toBe("2026-11-10T12:00:00Z");
		expect(checkout.needsYou && checkout.running).toBe(true);
		expect(parseRooms({ unchanged: true, snapshotId: "a1" }, "a1")).toEqual({ unchanged: true, snapshotId: "a1" });
		expect(parseRooms({ unchanged: true, snapshotId: "a1" }, "zz")).toBeNull();
		expect(parseRooms({ nope: 1 }, null)).toBeNull();
		expect(roomKey(null)).toBe("geral");
	});

	it("counts as news only an agent's line after the phone last opened the room", () => {
		const answered = room({ last: { agentId: "dev", agentName: "Dev", speaker: "agent", text: "Pronto.", at: "2026-11-10T12:00:00Z" } });
		expect(roomUnread(answered, {}, "2026-11-10T11:00:00Z")).toBe(true);
		expect(roomUnread(answered, {}, "2026-11-10T13:00:00Z")).toBe(false);
		expect(roomUnread(answered, { p: "2026-11-10T12:30:00Z" }, "2026-11-01T00:00:00Z")).toBe(false);
		expect(roomUnread(answered, {}, null)).toBe(false);
		const mine = room({ last: { ...answered.last!, speaker: "person" } });
		expect(roomUnread(mine, {}, "2026-11-10T11:00:00Z")).toBe(false);
	});

	it("says what the room is doing, loudest first", () => {
		const names = (id: string) => ({ dev: "Dev", qa: "QA" })[id] ?? null;
		expect(roomStatus(room({ needsYou: true, working: ["dev"] }), names)).toEqual({ tone: "wait", text: "Precisa de você" });
		expect(roomStatus(room({ working: ["dev"] }), names).text).toBe("Dev está trabalhando…");
		expect(roomStatus(room({ working: ["dev", "qa"] }), names).text).toBe("2 agentes trabalhando…");
		const last = { agentId: "dev", agentName: "Dev", speaker: "agent" as const, text: "Pronto.", at: "2026-11-10T12:00:00Z" };
		expect(roomStatus(room({ last }), names)).toEqual({ tone: "idle", text: "Dev: Pronto." });
		expect(roomStatus(room({ last: { ...last, speaker: "person" } }), names).text).toBe("Você: Pronto.");
		expect(roomStatus(room({ running: true }), names)).toEqual({ tone: "live", text: "Uma tarefa está rodando" });
		expect(roomStatus(room(), names).text).toBe("Ninguém falou ainda");
	});

	it("words the time like a messages list", () => {
		const now = new Date(2026, 10, 10, 15, 0);
		expect(roomTime(new Date(2026, 10, 10, 9, 41).toISOString(), now)).toBe("09:41");
		expect(roomTime(new Date(2026, 10, 9, 23, 0).toISOString(), now)).toBe("ontem");
		expect(roomTime(new Date(2026, 10, 7, 9, 0).toISOString(), now)).toBe("sáb");
		expect(roomTime(new Date(2026, 9, 12, 9, 0).toISOString(), now)).toBe("12/10");
		expect(roomTime(null, now)).toBe("");
		expect(roomTime("nope", now)).toBe("");
	});

	it("keeps Geral first and lifts the rooms that wait on the person", () => {
		const sorted = sortRooms([room({ id: "a", name: "A" }), room({ id: "b", name: "B", needsYou: true }), room({ id: null, name: "Geral" }), room({ id: "c", name: "C" })]);
		expect(sorted.map((r) => r.name)).toEqual(["Geral", "B", "A", "C"]);
	});
});
