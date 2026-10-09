import { describe, expect, it } from "vitest";
import { toHex } from "../src/bytes.ts";
import {
	fingerprint,
	formatPairingOffer,
	pairIdFromOtp,
	parsePairingOffer,
	pskFromOtp,
} from "../src/offer.ts";

describe("pairing offer", () => {
	it("derives the same values as dcc_remote::offer", () => {
		const otp = Uint8Array.from({ length: 16 }, (_, i) => i);
		expect(toHex(pskFromOtp(otp))).toBe(
			"4fc722feee1d0da95f23518afb4857dc3ad7173746506c13551d673d62e5075b",
		);
		expect(pairIdFromOtp(otp)).toBe("a3ad52c635e97f88");
		expect(fingerprint(new Uint8Array(32).fill(1))).toBe("72CD-6E84-22C4");
	});

	it("round-trips and parses the Rust form", () => {
		const offer = {
			relay: "wss://relay.example/",
			room: "AAAAAAAAAAAAAAAAAAAAAA",
			desktopPublicKey: new Uint8Array(32).fill(9),
			otp: new Uint8Array(16).fill(3),
		};
		const url = formatPairingOffer(offer);
		expect(url.startsWith("dcc://pair#v=1&relay=wss%3A%2F%2Frelay.example%2F&room=")).toBe(true);
		expect(parsePairingOffer(url)).toEqual(offer);
		expect(() => parsePairingOffer("dcc://pair#v=2")).toThrow();
		expect(() => parsePairingOffer(url.replace("relay=wss", "relay=https"))).toThrow();
	});
});
