import { describe, expect, it } from "vitest";
import { forwardableCloseCode, parseRoute, TokenBucket } from "../src/protocol.ts";

const ROOM = "AbCdEfGhIjKlMnOpQrSt_-";

describe("routes", () => {
	it("accepts the three connection kinds and nothing else", () => {
		expect(parseRoute(`/v1/rooms/${ROOM}/control`)).toEqual({ kind: "control", room: ROOM });
		expect(parseRoute(`/v1/rooms/${ROOM}/connect`)).toEqual({ kind: "connect", room: ROOM });
		const id = "0123456789abcdef0123456789abcdef";
		expect(parseRoute(`/v1/rooms/${ROOM}/data/${id}`)).toEqual({ kind: "data", room: ROOM, connectionId: id });
		expect(parseRoute(`/v1/rooms/short/connect`)).toBeNull();
		expect(parseRoute(`/v1/rooms/${ROOM}/data/xyz`)).toBeNull();
		expect(parseRoute(`/v1/rooms/${ROOM}/connect/extra`)).toBeNull();
		expect(parseRoute(`/debug/rooms/${ROOM}/traffic`)).toEqual({ kind: "traffic", room: ROOM });
	});
});

describe("limits", () => {
	it("throttles past the burst and refills over time", () => {
		const bucket = new TokenBucket(10, 3, 0);
		expect([bucket.take(0), bucket.take(0), bucket.take(0), bucket.take(0)]).toEqual([true, true, true, false]);
		expect(bucket.take(100)).toBe(true);
		expect(bucket.take(100)).toBe(false);
	});

	it("only forwards application close codes", () => {
		expect(forwardableCloseCode(4410)).toBe(4410);
		expect(forwardableCloseCode(1006)).toBe(1000);
	});
});
