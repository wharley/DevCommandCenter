/**
 * The phone side of the remote channel: connects to the relay, runs the Noise
 * handshake with the desktop and speaks the frame protocol over it.
 *
 * Works with any WebSocket that matches the browser API (browsers, React
 * Native, Node 22+).
 */
import { fromUtf8, utf8 } from "./bytes.ts";
import {
	CloseCode,
	decodeHandshakeResponse,
	encodeHandshakeInit,
	MODE_PAIR,
	MODE_RESUME,
	type Mode,
	SecureChannel,
} from "./channel.ts";
import {
	bodyBytes,
	bodyFields,
	decodeFrame,
	encodeFrame,
	type EventFrame,
	type Frame,
	type Headers,
	type ResponseFrame,
} from "./frames.ts";
import {
	HandshakeState,
	type KeyPair,
	PAIR_PATTERN,
	PROLOGUE,
	RESUME_PATTERN,
} from "./noise.ts";
import { fingerprint, pairIdFromOtp, type PairingOffer, pskFromOtp } from "./offer.ts";

export const PROTOCOL_VERSION = 1;

export type WebSocketLike = {
	binaryType: string;
	readonly readyState: number;
	send(data: Uint8Array): void;
	close(code?: number, reason?: string): void;
	onopen: ((event: unknown) => void) | null;
	onmessage: ((event: { data: unknown }) => void) | null;
	onclose: ((event: { code: number; reason: string }) => void) | null;
	onerror: ((event: unknown) => void) | null;
};

export type WebSocketFactory = (url: string) => WebSocketLike;

export type DeviceIdentity = {
	keyPair: KeyPair;
	name: string;
	platform?: string;
};

export type DesktopTarget = {
	relay: string;
	room: string;
	desktopPublicKey: Uint8Array;
};

export type ClientOptions = {
	webSocket?: WebSocketFactory;
	/** Time allowed for the relay connection and the handshake. */
	handshakeTimeoutMs?: number;
};

export type PairOptions = ClientOptions & {
	/** Called once the desktop shows the approval dialog. */
	onPending?: (info: { fingerprint: string }) => void;
	/** How long to wait for the person to approve on the desktop. */
	approvalTimeoutMs?: number;
};

export class RemoteChannelError extends Error {
	readonly code: number | null;

	constructor(message: string, code: number | null = null) {
		super(message);
		this.name = "RemoteChannelError";
		this.code = code;
	}
}

const CLOSE_MESSAGES: Record<number, string> = {
	[CloseCode.PROTOCOL]: "the desktop rejected the connection (protocol error)",
	[CloseCode.UNKNOWN_DEVICE]: "this device is not paired with the desktop",
	[CloseCode.PAIRING_DENIED]: "pairing was refused on the desktop",
	[CloseCode.OFFER_INVALID]: "the pairing code is invalid, expired or already used",
	[CloseCode.DEVICE_LIMIT]: "the desktop already has the maximum number of paired devices",
	[CloseCode.REVOKED]: "this device was revoked on the desktop",
};

export function relayUrl(relay: string, room: string, ...rest: string[]): string {
	return [relay.replace(/\/+$/, ""), "v1", "rooms", room, ...rest].join("/");
}

function defaultWebSocket(url: string): WebSocketLike {
	const Ctor = (globalThis as { WebSocket?: new (url: string) => WebSocketLike }).WebSocket;
	if (!Ctor) throw new Error("no WebSocket implementation available; pass options.webSocket");
	return new Ctor(url);
}

async function toBytes(data: unknown): Promise<Uint8Array> {
	if (data instanceof Uint8Array) return data;
	if (data instanceof ArrayBuffer) return new Uint8Array(data);
	if (ArrayBuffer.isView(data)) return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
	if (typeof Blob !== "undefined" && data instanceof Blob) {
		return new Uint8Array(await data.arrayBuffer());
	}
	throw new Error("unexpected text message on the relay channel");
}

export type RemoteResponse = {
	status: number;
	headers: Headers;
	body: Uint8Array;
	text(): string;
	json<T = unknown>(): T;
};

export type RequestInit = {
	headers?: Headers;
	/** A string or bytes is sent as is; anything else as JSON. */
	body?: unknown;
};

export type StreamHandlers = {
	onEvent: (event: EventFrame) => void;
	onEnd?: (error?: string) => void;
};

type Pending = { resolve: (frame: ResponseFrame) => void; reject: (error: Error) => void };

export class RemoteClient {
	private device: string | null;
	private readonly socket: WebSocketLike;
	private readonly channel: SecureChannel;
	private nextId = 1;
	private readonly pending = new Map<number, Pending>();
	private readonly streams = new Map<number, StreamHandlers>();
	private readonly pongs: Array<() => void> = [];
	private closed: RemoteChannelError | null = null;
	private readonly closeListeners = new Set<(error: RemoteChannelError) => void>();
	/** Frames that arrive before anyone listens for them (pairing). */
	private frameListener: ((frame: Frame) => void) | null = null;
	private inbound: Promise<void> = Promise.resolve();

	private constructor(socket: WebSocketLike, channel: SecureChannel, deviceId: string | null) {
		this.socket = socket;
		this.channel = channel;
		this.device = deviceId;
		socket.onmessage = (event) => {
			// Keep order even when a Blob needs an async read.
			this.inbound = this.inbound.then(() => this.receive(event.data));
		};
		socket.onclose = (event) => this.shutdown(closeError(event.code, event.reason));
		socket.onerror = () => {};
	}

	/** Reconnects an approved device (Noise IK). */
	static async connect(
		target: DesktopTarget,
		device: DeviceIdentity,
		options: ClientOptions = {},
	): Promise<RemoteClient> {
		const { socket, channel, deviceId } = await handshake(MODE_RESUME, target, device, null, options);
		return new RemoteClient(socket, channel, deviceId);
	}

	/**
	 * Pairs with the desktop from a scanned QR (Noise IKpsk2) and waits for the
	 * person to approve this device there. The returned client is ready to use.
	 */
	static async pair(
		offer: PairingOffer,
		device: DeviceIdentity,
		options: PairOptions = {},
	): Promise<RemoteClient> {
		const { socket, channel } = await handshake(MODE_PAIR, offer, device, offer.otp, options);
		const client = new RemoteClient(socket, channel, null);
		const approvalTimeout = options.approvalTimeoutMs ?? 180_000;
		const deviceId = await new Promise<string>((resolve, reject) => {
			const timer = setTimeout(() => {
				client.close();
				reject(new RemoteChannelError("timed out waiting for approval on the desktop"));
			}, approvalTimeout);
			const finish = () => {
				clearTimeout(timer);
				client.frameListener = null;
				client.closeListeners.delete(onClose);
			};
			const onClose = (error: RemoteChannelError) => {
				finish();
				reject(error);
			};
			client.closeListeners.add(onClose);
			client.frameListener = (frame) => {
				if (frame.t !== "pairing") return;
				if (frame.status === "pending") {
					options.onPending?.({ fingerprint: fingerprint(device.keyPair.publicKey) });
				} else if (frame.status === "approved" && frame.deviceId) {
					finish();
					resolve(frame.deviceId);
				} else {
					finish();
					client.close();
					reject(new RemoteChannelError(CLOSE_MESSAGES[CloseCode.PAIRING_DENIED]!, CloseCode.PAIRING_DENIED));
				}
			};
			// Proves to the desktop that this phone holds the QR secret.
			client.send({ t: "ping" });
		});
		client.device = deviceId;
		return client;
	}

	/** The id the desktop gave this device. */
	get deviceId(): string | null {
		return this.device;
	}

	get isOpen(): boolean {
		return this.closed === null;
	}

	onClose(listener: (error: RemoteChannelError) => void): () => void {
		this.closeListeners.add(listener);
		return () => this.closeListeners.delete(listener);
	}

	async request(method: string, path: string, init: RequestInit = {}): Promise<RemoteResponse> {
		const id = this.allocateId();
		const headers: Headers = { ...init.headers };
		let body: Uint8Array | string | undefined;
		if (typeof init.body === "string" || init.body instanceof Uint8Array) {
			body = init.body;
		} else if (init.body !== undefined) {
			body = JSON.stringify(init.body);
			headers["content-type"] ??= "application/json";
		}
		const frame = await new Promise<ResponseFrame>((resolve, reject) => {
			this.pending.set(id, { resolve, reject });
			try {
				this.send({ t: "req", id, method, path, headers, ...bodyFields(body) });
			} catch (error) {
				this.pending.delete(id);
				reject(error as Error);
			}
		});
		const bytes = bodyBytes(frame);
		return {
			status: frame.status,
			headers: frame.headers ?? {},
			body: bytes,
			text: () => fromUtf8(bytes),
			json: <T>() => JSON.parse(fromUtf8(bytes)) as T,
		};
	}

	/** Opens an SSE stream of the desktop API as `evt` frames. */
	subscribe(path: string, handlers: StreamHandlers): { id: number; cancel(): void } {
		const id = this.allocateId();
		this.streams.set(id, handlers);
		this.send({ t: "sub", id, path });
		return {
			id,
			cancel: () => {
				if (!this.streams.delete(id)) return;
				if (this.isOpen) this.send({ t: "cancel", id });
			},
		};
	}

	ping(): Promise<void> {
		return new Promise((resolve) => {
			this.pongs.push(resolve);
			this.send({ t: "ping", ts: Date.now() });
		});
	}

	close() {
		if (this.closed) return;
		try {
			this.socket.close(1000, "closed by the device");
		} catch {
			// already closing
		}
		this.shutdown(new RemoteChannelError("closed by the device", 1000));
	}

	private allocateId(): number {
		const id = this.nextId;
		this.nextId += 1;
		return id;
	}

	private send(frame: Frame) {
		if (this.closed) throw this.closed;
		for (const wire of this.channel.seal(encodeFrame(frame))) this.socket.send(wire);
	}

	private async receive(data: unknown) {
		if (this.closed) return;
		let frame: Frame;
		try {
			const message = this.channel.open(await toBytes(data));
			if (message === null) return;
			frame = decodeFrame(message);
		} catch (error) {
			this.socket.close(CloseCode.PROTOCOL, "bad frame");
			this.shutdown(new RemoteChannelError(`channel error: ${(error as Error).message}`));
			return;
		}
		this.dispatch(frame);
	}

	private dispatch(frame: Frame) {
		switch (frame.t) {
			case "res": {
				const pending = this.pending.get(frame.id);
				this.pending.delete(frame.id);
				pending?.resolve(frame);
				return;
			}
			case "evt":
				this.streams.get(frame.id)?.onEvent(frame);
				return;
			case "end": {
				const stream = this.streams.get(frame.id);
				this.streams.delete(frame.id);
				stream?.onEnd?.(frame.error);
				return;
			}
			case "ping":
				this.send({ t: "pong", ts: frame.ts });
				return;
			case "pong":
				this.pongs.shift()?.();
				return;
			default:
				this.frameListener?.(frame);
		}
	}

	private shutdown(error: RemoteChannelError) {
		if (this.closed) return;
		this.closed = error;
		for (const pending of this.pending.values()) pending.reject(error);
		this.pending.clear();
		for (const stream of this.streams.values()) stream.onEnd?.(error.message);
		this.streams.clear();
		for (const listener of [...this.closeListeners]) listener(error);
	}
}

function closeError(code: number, reason: string): RemoteChannelError {
	const known = CLOSE_MESSAGES[code];
	return new RemoteChannelError(known ?? `connection closed (${code}${reason ? `: ${reason}` : ""})`, code);
}

async function handshake(
	mode: Mode,
	target: DesktopTarget,
	device: DeviceIdentity,
	otp: Uint8Array | null,
	options: ClientOptions,
): Promise<{ socket: WebSocketLike; channel: SecureChannel; deviceId: string | null }> {
	const state = new HandshakeState({
		protocolName: mode === MODE_PAIR ? PAIR_PATTERN : RESUME_PATTERN,
		initiator: true,
		prologue: PROLOGUE,
		localStatic: device.keyPair,
		remoteStatic: target.desktopPublicKey,
		psk: otp ? pskFromOtp(otp) : undefined,
	});
	const hello = {
		v: PROTOCOL_VERSION,
		...(otp ? { pairId: pairIdFromOtp(otp) } : {}),
		device: { name: device.name, ...(device.platform ? { platform: device.platform } : {}) },
	};
	const socket = (options.webSocket ?? defaultWebSocket)(relayUrl(target.relay, target.room, "connect"));
	socket.binaryType = "arraybuffer";
	const timeoutMs = options.handshakeTimeoutMs ?? 15_000;

	return new Promise((resolve, reject) => {
		let settled = false;
		const fail = (error: RemoteChannelError) => {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			try {
				socket.close(1000);
			} catch {
				// already closed
			}
			reject(error);
		};
		const timer = setTimeout(
			() => fail(new RemoteChannelError("timed out connecting to the desktop")),
			timeoutMs,
		);
		socket.onopen = () => {
			try {
				socket.send(encodeHandshakeInit(mode, state.writeMessage(utf8(JSON.stringify(hello)))));
			} catch (error) {
				fail(new RemoteChannelError((error as Error).message));
			}
		};
		socket.onerror = () => {};
		socket.onclose = (event) => fail(closeError(event.code, event.reason));
		socket.onmessage = (event) => {
			socket.onmessage = null;
			void (async () => {
				try {
					const payload = state.readMessage(decodeHandshakeResponse(await toBytes(event.data)));
					const server = JSON.parse(fromUtf8(payload)) as { v?: number; deviceId?: string };
					if (server.v !== PROTOCOL_VERSION) {
						throw new Error(`desktop speaks protocol ${String(server.v)}`);
					}
					if (settled) return;
					settled = true;
					clearTimeout(timer);
					resolve({
						socket,
						channel: new SecureChannel(state.split()),
						deviceId: server.deviceId ?? null,
					});
				} catch (error) {
					fail(new RemoteChannelError(`handshake failed: ${(error as Error).message}`));
				}
			})();
		};
	});
}
