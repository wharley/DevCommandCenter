import { DurableObject } from "cloudflare:workers";
import {
	constantTimeEqual,
	forwardableCloseCode,
	MAX_BUFFERED_BYTES,
	MAX_BUFFERED_FRAMES,
	MAX_FRAME_BYTES,
	MAX_PHONES_PER_ROOM,
	parseRoute,
	PHONE_FRAME_BURST,
	PHONE_FRAMES_PER_SECOND,
	RelayClose,
	ROOM_TTL_MS,
	sha256Hex,
	TokenBucket,
} from "./protocol.ts";

export type Env = {
	ROOMS: DurableObjectNamespace<RelayRoom>;
	DCC_RELAY_DEBUG_TRAFFIC?: string;
};

type Attachment =
	| { role: "control" }
	| { role: "phone"; id: string }
	| { role: "data"; id: string }
	| { role: "rejected" };

type TrafficEntry = {
	at: number;
	direction: "phone-to-desktop" | "desktop-to-phone";
	connectionId: string;
	bytes: number;
	base64: string;
};

const ROOM_SECRET = /^[A-Za-z0-9_-]{32,128}$/;
const MAX_TRAFFIC_ENTRIES = 256;

function base64(bytes: ArrayBuffer): string {
	let binary = "";
	const view = new Uint8Array(bytes);
	for (let i = 0; i < view.length; i++) binary += String.fromCharCode(view[i]!);
	return btoa(binary);
}

function randomConnectionId(): string {
	const bytes = crypto.getRandomValues(new Uint8Array(16));
	return [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function safeClose(socket: WebSocket, code: number, reason = "") {
	try {
		socket.close(code, reason.slice(0, 120));
	} catch {
		// already closed
	}
}

function closeCodeFor(code: number): number {
	return code === 1000 || (code >= 3000 && code <= 4999) ? code : 1000;
}

/**
 * One room per desktop. Uses WebSocket Hibernation: idle connections cost
 * nothing and the object wakes up only to forward a frame. Holds no payload
 * beyond the frames a phone sends before the desktop's data connection is up
 * (kept in memory, lost if the object is evicted; the phone retries).
 */
export class RelayRoom extends DurableObject<Env> {
	private readonly buckets = new Map<string, TokenBucket>();
	private readonly buffers = new Map<string, { frames: ArrayBuffer[]; bytes: number }>();
	private readonly traffic: TrafficEntry[] = [];

	constructor(ctx: DurableObjectState, env: Env) {
		super(ctx, env);
		// Keep-alives from the desktop are answered without waking the object.
		ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair("ping", "pong"));
	}

	override async fetch(request: Request): Promise<Response> {
		const route = parseRoute(new URL(request.url).pathname);
		if (!route) return new Response("not found", { status: 404 });
		switch (route.kind) {
			case "control":
				return this.openControl(request);
			case "connect":
				return this.openPhone();
			case "data":
				return this.openData(request, route.connectionId);
			case "traffic":
				if (this.env.DCC_RELAY_DEBUG_TRAFFIC !== "1") {
					return new Response("not found", { status: 404 });
				}
				return Response.json({ entries: this.traffic });
		}
	}

	private async checkSecret(request: Request, register: boolean): Promise<boolean> {
		const secret = request.headers.get("x-dcc-room-secret") ?? "";
		if (!ROOM_SECRET.test(secret)) return false;
		const hash = await sha256Hex(secret);
		const stored = await this.ctx.storage.get<string>("secretHash");
		if (!stored) {
			if (!register) return false;
			await this.ctx.storage.put("secretHash", hash);
			return true;
		}
		return constantTimeEqual(stored, hash);
	}

	private accept(tags: string[], attachment: Attachment): Response {
		const pair = new WebSocketPair();
		const [client, server] = [pair[0], pair[1]];
		this.ctx.acceptWebSocket(server, tags);
		server.serializeAttachment(attachment);
		return new Response(null, { status: 101, webSocket: client });
	}

	private sockets(tag: string): WebSocket[] {
		return this.ctx.getWebSockets(tag);
	}

	private async openControl(request: Request): Promise<Response> {
		if (!(await this.checkSecret(request, true))) {
			return new Response("forbidden", { status: 403 });
		}
		// A new control connection replaces the old one; phones reconnect.
		for (const socket of this.sockets("control")) safeClose(socket, RelayClose.REPLACED, "replaced");
		for (const socket of [...this.sockets("phone"), ...this.sockets("data")]) {
			safeClose(socket, RelayClose.DESKTOP_OFFLINE, "desktop reconnected");
		}
		this.buffers.clear();
		const now = Date.now();
		await this.ctx.storage.put("lastControlAt", now);
		await this.ctx.storage.setAlarm(now + ROOM_TTL_MS);
		return this.accept(["control"], { role: "control" });
	}

	/** Accepts and closes at once, so the phone gets a close code it can show. */
	private reject(code: number, reason: string): Response {
		const response = this.accept(["rejected"], { role: "rejected" });
		for (const socket of this.sockets("rejected")) safeClose(socket, code, reason);
		return response;
	}

	private openPhone(): Response {
		const control = this.sockets("control")[0];
		if (!control) return this.reject(RelayClose.DESKTOP_OFFLINE, "desktop offline");
		if (this.sockets("phone").length >= MAX_PHONES_PER_ROOM) {
			return this.reject(RelayClose.TOO_MANY_CONNECTIONS, "too many connections");
		}
		const id = randomConnectionId();
		const response = this.accept(["phone", `phone:${id}`], { role: "phone", id });
		this.buckets.set(id, new TokenBucket(PHONE_FRAMES_PER_SECOND, PHONE_FRAME_BURST, Date.now()));
		control.send(JSON.stringify({ type: "connected", connectionId: id }));
		return response;
	}

	private async openData(request: Request, id: string): Promise<Response> {
		if (!(await this.checkSecret(request, false))) {
			return new Response("forbidden", { status: 403 });
		}
		if (this.sockets(`phone:${id}`).length === 0) {
			return new Response("no such connection", { status: 404 });
		}
		for (const socket of this.sockets(`data:${id}`)) safeClose(socket, RelayClose.REPLACED, "replaced");
		const response = this.accept(["data", `data:${id}`], { role: "data", id });
		const [data] = this.sockets(`data:${id}`);
		const buffered = this.buffers.get(id);
		this.buffers.delete(id);
		if (data && buffered) for (const frame of buffered.frames) data.send(frame);
		return response;
	}

	private record(direction: TrafficEntry["direction"], connectionId: string, frame: ArrayBuffer) {
		if (this.env.DCC_RELAY_DEBUG_TRAFFIC !== "1") return;
		this.traffic.push({
			at: Date.now(),
			direction,
			connectionId,
			bytes: frame.byteLength,
			base64: base64(frame),
		});
		if (this.traffic.length > MAX_TRAFFIC_ENTRIES) this.traffic.shift();
	}

	override async webSocketMessage(socket: WebSocket, message: string | ArrayBuffer) {
		const attachment = socket.deserializeAttachment() as Attachment | null;
		if (!attachment || attachment.role === "control" || attachment.role === "rejected") return;
		if (typeof message === "string") {
			safeClose(socket, RelayClose.BAD_FRAME, "binary frames only");
			return;
		}
		if (message.byteLength > MAX_FRAME_BYTES) {
			safeClose(socket, RelayClose.FRAME_TOO_LARGE, "frame too large");
			return;
		}
		const { id } = attachment;
		if (attachment.role === "phone") {
			let bucket = this.buckets.get(id);
			if (!bucket) {
				bucket = new TokenBucket(PHONE_FRAMES_PER_SECOND, PHONE_FRAME_BURST, Date.now());
				this.buckets.set(id, bucket);
			}
			if (!bucket.take(Date.now())) {
				safeClose(socket, RelayClose.RATE_LIMITED, "rate limited");
				return;
			}
			this.record("phone-to-desktop", id, message);
			const [data] = this.sockets(`data:${id}`);
			if (data) {
				data.send(message);
				return;
			}
			const buffer = this.buffers.get(id) ?? { frames: [], bytes: 0 };
			if (
				buffer.frames.length >= MAX_BUFFERED_FRAMES ||
				buffer.bytes + message.byteLength > MAX_BUFFERED_BYTES
			) {
				safeClose(socket, RelayClose.RATE_LIMITED, "desktop not ready");
				return;
			}
			buffer.frames.push(message);
			buffer.bytes += message.byteLength;
			this.buffers.set(id, buffer);
			return;
		}
		this.record("desktop-to-phone", id, message);
		for (const phone of this.sockets(`phone:${id}`)) phone.send(message);
	}

	override async webSocketClose(socket: WebSocket, code: number, reason: string) {
		this.closed(socket, code, reason);
		safeClose(socket, closeCodeFor(code), reason);
	}

	override async webSocketError(socket: WebSocket) {
		this.closed(socket, 1011, "");
	}

	private closed(socket: WebSocket, code: number, reason: string) {
		const attachment = socket.deserializeAttachment() as Attachment | null;
		if (!attachment) return;
		switch (attachment.role) {
			case "rejected":
				return;
			case "control":
				if (this.sockets("control").some((other) => other !== socket)) return;
				for (const other of [...this.sockets("phone"), ...this.sockets("data")]) {
					safeClose(other, RelayClose.DESKTOP_OFFLINE, "desktop offline");
				}
				this.buffers.clear();
				return;
			case "phone": {
				this.buckets.delete(attachment.id);
				this.buffers.delete(attachment.id);
				for (const data of this.sockets(`data:${attachment.id}`)) {
					safeClose(data, 1000, "phone left");
				}
				for (const control of this.sockets("control")) {
					try {
						control.send(JSON.stringify({ type: "disconnected", connectionId: attachment.id }));
					} catch {
						// control going away too
					}
				}
				return;
			}
			case "data":
				// The desktop's close code (e.g. "pairing refused") reaches the phone.
				for (const phone of this.sockets(`phone:${attachment.id}`)) {
					safeClose(phone, forwardableCloseCode(code), reason);
				}
				return;
		}
	}

	override async alarm() {
		const lastControlAt = (await this.ctx.storage.get<number>("lastControlAt")) ?? 0;
		if (this.sockets("control").length > 0) {
			await this.ctx.storage.put("lastControlAt", Date.now());
			await this.ctx.storage.setAlarm(Date.now() + ROOM_TTL_MS);
			return;
		}
		if (Date.now() - lastControlAt >= ROOM_TTL_MS) {
			await this.ctx.storage.deleteAll();
			return;
		}
		await this.ctx.storage.setAlarm(lastControlAt + ROOM_TTL_MS);
	}
}
