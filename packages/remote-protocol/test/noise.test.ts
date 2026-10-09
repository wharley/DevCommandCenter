import { readFileSync } from "node:fs";
import { sha256 } from "@noble/hashes/sha2.js";
import { describe, expect, it } from "vitest";
import { fromHex, fromUtf8, toHex, utf8 } from "../src/bytes.ts";
import { encodeHandshakeInit, MAX_CHUNK, type Mode, SecureChannel } from "../src/channel.ts";
import { HandshakeState, keyPairFromPrivate, PROLOGUE } from "../src/noise.ts";
import { pairIdFromOtp, pskFromOtp } from "../src/offer.ts";

function readVectors<T>(name: string): T {
	return JSON.parse(readFileSync(new URL(`./vectors/${name}`, import.meta.url), "utf8")) as T;
}

type CacophonyVector = {
	protocol_name: string;
	init_prologue: string;
	init_psks?: string[];
	init_static: string;
	init_ephemeral: string;
	init_remote_static: string;
	resp_static: string;
	resp_ephemeral: string;
	handshake_hash: string;
	messages: Array<{ payload: string; ciphertext: string }>;
};

describe("cacophony vectors", () => {
	const { vectors } = readVectors<{ vectors: CacophonyVector[] }>("cacophony.json");

	it("covers both patterns DCC uses", () => {
		expect(vectors.map((v) => v.protocol_name).sort()).toEqual([
			"Noise_IK_25519_ChaChaPoly_SHA256",
			"Noise_IKpsk2_25519_ChaChaPoly_SHA256",
		]);
	});

	for (const vector of vectors) {
		it(vector.protocol_name, () => {
			const psk = vector.init_psks?.[0] ? fromHex(vector.init_psks[0]) : undefined;
			const initiator = new HandshakeState({
				protocolName: vector.protocol_name,
				initiator: true,
				prologue: fromHex(vector.init_prologue),
				localStatic: keyPairFromPrivate(fromHex(vector.init_static)),
				localEphemeral: keyPairFromPrivate(fromHex(vector.init_ephemeral)),
				remoteStatic: fromHex(vector.init_remote_static),
				psk,
			});
			const responder = new HandshakeState({
				protocolName: vector.protocol_name,
				initiator: false,
				prologue: fromHex(vector.init_prologue),
				localStatic: keyPairFromPrivate(fromHex(vector.resp_static)),
				localEphemeral: keyPairFromPrivate(fromHex(vector.resp_ephemeral)),
				psk,
			});
			const [first, second, ...transport] = vector.messages;
			const message1 = initiator.writeMessage(fromHex(first!.payload));
			expect(toHex(message1)).toBe(first!.ciphertext);
			expect(toHex(responder.readMessage(message1))).toBe(first!.payload);
			const message2 = responder.writeMessage(fromHex(second!.payload));
			expect(toHex(message2)).toBe(second!.ciphertext);
			expect(toHex(initiator.readMessage(message2))).toBe(second!.payload);
			expect(toHex(initiator.handshakeHash)).toBe(vector.handshake_hash);
			expect(toHex(responder.handshakeHash)).toBe(vector.handshake_hash);

			const i = initiator.split();
			const r = responder.split();
			transport.forEach((message, index) => {
				const [send, receive] = index % 2 === 0 ? [i.send, r.receive] : [r.send, i.receive];
				const ciphertext = send.encryptWithAd(new Uint8Array(0), fromHex(message.payload));
				expect(toHex(ciphertext)).toBe(message.ciphertext);
				expect(toHex(receive.decryptWithAd(new Uint8Array(0), ciphertext))).toBe(message.payload);
			});
		});
	}
});

type WireDescription = { hex: string } | { len: number; sha256: string; headHex: string };
type Step =
	| { from: "phone" | "desktop"; handshake: WireDescription }
	| {
			from: "phone" | "desktop";
			message: { text: string } | { generated: number };
			wires: WireDescription[];
	  };
type InteropCase = {
	name: string;
	mode: Mode;
	protocolName: string;
	prologue: string;
	otp: string;
	psk: string;
	pairId: string;
	phoneStaticPrivate: string;
	phoneStaticPublic: string;
	phoneEphemeralPrivate: string;
	desktopStaticPrivate: string;
	desktopStaticPublic: string;
	desktopEphemeralPrivate: string;
	clientHello: string;
	serverHello: string;
	handshakeHash: string;
	steps: Step[];
};

function expectWire(actual: Uint8Array, expected: WireDescription) {
	if ("hex" in expected) {
		expect(toHex(actual)).toBe(expected.hex);
	} else {
		expect(actual.length).toBe(expected.len);
		expect(toHex(actual.subarray(0, 32))).toBe(expected.headHex);
		expect(toHex(sha256(actual))).toBe(expected.sha256);
	}
}

function generated(length: number): Uint8Array {
	const out = new Uint8Array(length);
	for (let i = 0; i < length; i++) out[i] = (i * 31 + 7) % 251;
	return out;
}

describe("snow interop vectors", () => {
	const { cases } = readVectors<{ cases: InteropCase[] }>("snow-interop.json");

	for (const vector of cases) {
		it(`reproduces snow byte for byte: ${vector.name}`, () => {
			const pairing = vector.mode === 1;
			const otp = fromHex(vector.otp);
			expect(toHex(pskFromOtp(otp))).toBe(vector.psk);
			expect(pairIdFromOtp(otp)).toBe(vector.pairId);
			expect(toHex(PROLOGUE)).toBe(vector.prologue);
			const phoneStatic = keyPairFromPrivate(fromHex(vector.phoneStaticPrivate));
			const desktopStatic = keyPairFromPrivate(fromHex(vector.desktopStaticPrivate));
			expect(toHex(phoneStatic.publicKey)).toBe(vector.phoneStaticPublic);
			expect(toHex(desktopStatic.publicKey)).toBe(vector.desktopStaticPublic);

			const phone = new HandshakeState({
				protocolName: vector.protocolName,
				initiator: true,
				prologue: PROLOGUE,
				localStatic: phoneStatic,
				localEphemeral: keyPairFromPrivate(fromHex(vector.phoneEphemeralPrivate)),
				remoteStatic: desktopStatic.publicKey,
				psk: pairing ? pskFromOtp(otp) : undefined,
			});
			const desktop = new HandshakeState({
				protocolName: vector.protocolName,
				initiator: false,
				prologue: PROLOGUE,
				localStatic: desktopStatic,
				localEphemeral: keyPairFromPrivate(fromHex(vector.desktopEphemeralPrivate)),
			});

			const [init, response, ...transport] = vector.steps;
			const message1 = phone.writeMessage(utf8(vector.clientHello));
			expectWire(encodeHandshakeInit(vector.mode, message1), (init as { handshake: WireDescription }).handshake);
			expect(fromUtf8(desktop.readMessage(message1))).toBe(vector.clientHello);
			expect(toHex(desktop.remoteStatic!)).toBe(vector.phoneStaticPublic);
			// Like the desktop, pick the PSK only after reading message 1.
			if (pairing) desktop.setPsk(pskFromOtp(otp));
			const message2 = desktop.writeMessage(utf8(vector.serverHello));
			expectWire(
				Uint8Array.of(0x02, ...message2),
				(response as { handshake: WireDescription }).handshake,
			);
			expect(fromUtf8(phone.readMessage(message2))).toBe(vector.serverHello);
			expect(toHex(phone.handshakeHash)).toBe(vector.handshakeHash);

			const phoneChannel = new SecureChannel(phone.split());
			const desktopChannel = new SecureChannel(desktop.split());
			for (const step of transport) {
				if (!("message" in step)) throw new Error("unexpected handshake step");
				const bytes =
					"text" in step.message ? utf8(step.message.text) : generated(step.message.generated);
				const [sender, receiver] =
					step.from === "phone" ? [phoneChannel, desktopChannel] : [desktopChannel, phoneChannel];
				const wires = sender.seal(bytes);
				expect(wires.length).toBe(step.wires.length);
				let opened: Uint8Array | null = null;
				wires.forEach((wire, index) => {
					expectWire(wire, step.wires[index]!);
					opened = receiver.open(wire);
				});
				expect(opened).not.toBeNull();
				expect(toHex(opened!)).toBe(toHex(bytes));
			}
		});
	}

	it("uses chunks of the size snow expects", () => {
		expect(MAX_CHUNK).toBe(65518);
	});
});
