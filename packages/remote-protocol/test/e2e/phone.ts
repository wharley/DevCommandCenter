/**
 * The "phone" of the end-to-end test (`src-tauri/tests/remote_relay_e2e.rs`,
 * run by `scripts/e2e-remote.mjs`). Talks to the desktop only through the
 * relay, with this package's client.
 *
 * Config comes as JSON in `DCC_E2E_CONFIG`. Lines starting with `@@` on
 * stdout ask the desktop side to act; it answers `go` on stdin.
 */
import { createInterface } from "node:readline";
import {
	CloseCode,
	type EventFrame,
	fingerprint,
	generateKeyPair,
	parsePairingOffer,
	RemoteChannelError,
	RemoteClient,
} from "../../src/index.ts";

type Config = {
	offers: { approve: string; deny: string; expired: string };
	workspaceId: string;
	sessionId: string;
	replayAfter: number;
	resolvedRequestId: string;
	pendingRequestId: string;
};

const config = JSON.parse(process.env.DCC_E2E_CONFIG ?? "{}") as Config;
const lines = createInterface({ input: process.stdin });
const answers: string[] = [];
const waiters: Array<(line: string) => void> = [];
lines.on("line", (line) => {
	const waiter = waiters.shift();
	if (waiter) waiter(line);
	else answers.push(line);
});

function ask(command: string, payload: unknown = {}): Promise<string> {
	process.stdout.write(`@@${command} ${JSON.stringify(payload)}\n`);
	return new Promise((resolve) => {
		const queued = answers.shift();
		if (queued !== undefined) resolve(queued);
		else waiters.push(resolve);
	});
}

function log(step: string, detail: unknown = "") {
	process.stdout.write(`[phone] ${step} ${typeof detail === "string" ? detail : JSON.stringify(detail)}\n`);
}

function check(condition: unknown, message: string): asserts condition {
	if (!condition) throw new Error(`check failed: ${message}`);
}

async function expectRefused(promise: Promise<unknown>, code: number, what: string) {
	try {
		await promise;
	} catch (error) {
		check(error instanceof RemoteChannelError, `${what}: unexpected error ${String(error)}`);
		check(error.code === code, `${what}: expected close ${code}, got ${error.code} (${error.message})`);
		log("refused as expected", { what, code });
		return;
	}
	throw new Error(`${what}: was accepted`);
}

const approveOffer = parsePairingOffer(config.offers.approve);
const target = {
	relay: approveOffer.relay,
	room: approveOffer.room,
	desktopPublicKey: approveOffer.desktopPublicKey,
};
const phoneA = { keyPair: generateKeyPair(), name: "approve-phone-A", platform: "node" };
const phoneB = { keyPair: generateKeyPair(), name: "approve-phone-B", platform: "node" };
const stranger = { keyPair: generateKeyPair(), name: "deny-phone", platform: "node" };

// 1. Pair with approval.
let pendingFingerprint = "";
const paired = await RemoteClient.pair(approveOffer, phoneA, {
	onPending: (info) => {
		pendingFingerprint = info.fingerprint;
	},
});
check(paired.deviceId, "approved pairing returns a device id");
check(pendingFingerprint === fingerprint(phoneA.keyPair.publicKey), "pending shows our fingerprint");
log("paired", { deviceId: paired.deviceId, fingerprint: pendingFingerprint });
const firstList = await paired.request("GET", `/api/v1/sessions?workspaceId=${config.workspaceId}`);
check(firstList.status === 200, `list on the pairing channel: ${firstList.status}`);
paired.close();

// 2. The QR secret is single use, expires, and the person can refuse.
await expectRefused(RemoteClient.pair(approveOffer, phoneB), CloseCode.OFFER_INVALID, "reused otp");
await expectRefused(
	RemoteClient.pair(parsePairingOffer(config.offers.expired), phoneB),
	CloseCode.OFFER_INVALID,
	"expired otp",
);
await expectRefused(
	RemoteClient.pair(parsePairingOffer(config.offers.deny), stranger),
	CloseCode.PAIRING_DENIED,
	"refused on the desktop",
);

// 3. Only approved keys reconnect.
await expectRefused(RemoteClient.connect(target, phoneB), CloseCode.UNKNOWN_DEVICE, "unpaired key");
await expectRefused(RemoteClient.connect(target, stranger), CloseCode.UNKNOWN_DEVICE, "refused key");
const client = await RemoteClient.connect(target, phoneA);
check(client.deviceId === paired.deviceId, "IK reconnect keeps the device id");
log("reconnected with IK", { deviceId: client.deviceId });
await client.ping();

// 4. List sessions.
const list = await client.request("GET", `/api/v1/sessions?workspaceId=${config.workspaceId}`);
check(list.status === 200, `list sessions: ${list.status}`);
const sessions = list.json<Array<{ sessionId?: string; id?: string }>>();
check(
	JSON.stringify(sessions).includes(config.sessionId),
	`session list includes ${config.sessionId}: ${list.text().slice(0, 200)}`,
);
log("listed sessions", { count: sessions.length });

// 5. The full history is larger than one Noise message (fragmentation).
const history = await client.request("GET", `/api/v1/sessions/${config.sessionId}/events`);
check(history.status === 200, `history: ${history.status}`);
check(history.body.length > 65535, `history spans fragments: ${history.body.length} bytes`);
const delta = await client.request(
	"GET",
	`/api/v1/sessions/${config.sessionId}/events?after=0&limit=2000`,
);
const { lastSeq } = delta.json<{ lastSeq: number }>();
log("history", { bytes: history.body.length, lastSeq });

// 6. Subscribe with a cursor: replay after it, then live, without overlap.
const events: EventFrame[] = [];
let ended: string | undefined | null = null;
const subscription = client.subscribe(
	`/api/v1/events/stream?sessionId=${encodeURIComponent(config.sessionId)}&after=${config.replayAfter}`,
	{
		onEvent: (event) => events.push(event),
		onEnd: (error) => {
			ended = error;
		},
	},
);
const until = async (predicate: () => boolean, what: string, ms = 10_000) => {
	const start = Date.now();
	while (!predicate()) {
		if (Date.now() - start > ms) throw new Error(`timed out waiting for ${what}`);
		await new Promise((resolve) => setTimeout(resolve, 25));
	}
};
await until(
	() => events.filter((event) => event.event === "replay").length === lastSeq - config.replayAfter,
	"replay",
);
const replayed = events.filter((event) => event.event === "replay").map((event) => event.seq!);
check(
	replayed.every((seq, index) => seq === config.replayAfter + 1 + index),
	`replay in order after the cursor: ${replayed.join(",")}`,
);
log("replayed", { from: replayed[0], to: replayed.at(-1) });

const liveStart = events.length;
await ask("live", { after: lastSeq });
await until(
	() =>
		events
			.slice(liveStart)
			.some((event) => JSON.stringify(event.data).includes("sessionTurnPermissionRequested")),
	"live permission request",
);
const live = events.slice(liveStart);
const deltas = live.filter((event) => JSON.stringify(event.data).includes("sessionTurnAssistantMessageDelta"));
const text = deltas
	.map((event) => (event.data as { sessionTurnAssistantMessageDelta: { content: string } }).sessionTurnAssistantMessageDelta.content)
	.join("");
check(text === "Hello from the desktop!", `deltas carry the full text: ${JSON.stringify(text)}`);
check(deltas.length < 5, `five deltas were coalesced into ${deltas.length} frame(s)`);
const liveSeqs = live.map((event) => event.seq).filter((seq): seq is number => seq !== undefined);
check(liveSeqs.every((seq) => seq > lastSeq), `live events come after the replay: ${liveSeqs.join(",")}`);
check(
	liveSeqs.every((seq, index) => index === 0 || seq > liveSeqs[index - 1]!),
	`live sequences increase: ${liveSeqs.join(",")}`,
);
const permission = live.at(-1)!;
log("live", { frames: live.length, deltaFrames: deltas.length, permissionSeq: permission.seq });

// 7. Answer permissions, as the desktop would.
const already = await client.request(
	"POST",
	`/api/v1/sessions/${config.sessionId}/respond-permission`,
	{ body: { sessionId: "", requestId: config.resolvedRequestId, behavior: "deny" } },
);
check(already.status === 409, `answered request is a conflict: ${already.status} ${already.text()}`);
check(
	already.json<{ error: { outcome: { behavior: string } } }>().error.outcome.behavior === "allow",
	"the conflict carries the answer that won",
);
const answer = await client.request(
	"POST",
	`/api/v1/sessions/${config.sessionId}/respond-permission`,
	{ body: { sessionId: "", requestId: config.pendingRequestId, behavior: "allow" } },
);
log("answered the live permission", { status: answer.status, body: answer.text().slice(0, 200) });
const outcome = await ask("permission-answered", { status: answer.status, body: answer.text() });
check(outcome === "go", `desktop saw the answer: ${outcome}`);

// 8. Large request body (phone -> desktop fragmentation).
const big = await client.request(
	"POST",
	`/api/v1/sessions/${config.sessionId}/respond-user-input`,
	{ body: { sessionId: "", requestId: "missing", answers: { note: "x".repeat(150_000) } } },
);
// 422 = the desktop parsed the whole body as JSON (wrong shape on purpose).
check(big.status === 422, `large request reassembled on the desktop: ${big.status}`);
log("large request", { status: big.status });

// 9. Routes a phone must not reach.
for (const [method, path] of [
	["GET", "/auth/devices"],
	["POST", "/auth/pair-init"],
	["POST", "/api/v1/remote/pairing/offers"],
	["POST", "/api/v1/auth/bearer/rotate"],
	["POST", "/rpc"],
] as const) {
	const response = await client.request(method, path);
	check(response.status === 403, `${method} ${path} is forbidden remotely: ${response.status}`);
}
log("operator routes refused");

// 10. Revoking drops the live connection, and the key cannot come back.
subscription.cancel();
check(ended === null, "the stream was still open before cancelling");
const closed = new Promise<RemoteChannelError>((resolve) => client.onClose(resolve));
await ask("revoke", { deviceId: client.deviceId });
const closeError = await closed;
check(closeError.code === CloseCode.REVOKED, `revocation closes with ${CloseCode.REVOKED}: ${closeError.code}`);
await expectRefused(RemoteClient.connect(target, phoneA), CloseCode.UNKNOWN_DEVICE, "revoked key");

process.stdout.write(`@@done ${JSON.stringify({ deviceId: paired.deviceId, fingerprint: pendingFingerprint })}\n`);
lines.close();
process.exit(0);
