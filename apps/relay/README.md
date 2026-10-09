# DCC relay

Lets the DCC app on a phone reach a desktop from any network, without opening
ports, Tailscale or an account. The desktop makes an outbound WebSocket to the
relay; the phone connects to the same room; the relay forwards bytes between
them.

The relay never sees content. Phone and desktop run a Noise handshake
(`Noise_IKpsk2_25519_ChaChaPoly_SHA256` to pair, `Noise_IK_25519_ChaChaPoly_SHA256`
to reconnect) and everything after it is encrypted end to end
(`crates/dcc-remote`, `packages/remote-protocol`). What the relay learns is the
room id, IP addresses, timing and frame sizes.

## How it works

One Cloudflare Durable Object per room, using WebSocket Hibernation: idle
connections cost nothing and the object only wakes to forward a frame.

| Route (WebSocket) | Who | Credential |
| --- | --- | --- |
| `/v1/rooms/<room>/control` | desktop, one per room | `x-dcc-room-secret`; the first connection registers the room, the object keeps only the SHA-256 of the secret |
| `/v1/rooms/<room>/connect` | phone | none: the desktop authenticates it with Noise |
| `/v1/rooms/<room>/data/<connectionId>` | desktop, one per phone | `x-dcc-room-secret` |

When a phone connects, the relay sends the control connection
`{"type":"connected","connectionId":"…"}` and the desktop opens the matching
data connection. Frames the phone sends before that are held briefly (16
frames / 256 KiB). When either side closes, the other is closed; a close code
from the desktop in 4000–4999 (for example "pairing refused") reaches the
phone.

Room ids are 128 random bits (22 base64url characters).

## Limits

- Binary frames only, at most 65 537 bytes (one Noise message plus the
  envelope). Text from a phone closes it.
- 8 phones per room.
- Phones: 100 frames/s, bursts of 300. The room owner is not throttled.
- A room with no control connection for 30 days is deleted.
- Nothing is logged about payloads.

Close codes from the relay: 4600 bad frame, 4601 forbidden, 4603 desktop
offline, 4609 replaced by a newer connection, 4613 frame too large, 4628 rate
limited, 4629 too many connections.

## Running locally

```bash
yarn workspace @dcc/relay dev
```

`wrangler dev` runs everything on this machine; it needs no Cloudflare
account. Point the desktop at it with `DCC_RELAY_URL=ws://127.0.0.1:8787` or
in Settings → Devices (plain `ws://` is accepted only for localhost).

`DCC_RELAY_DEBUG_TRAFFIC=1` keeps the last frames of each room in memory and
serves them at `/debug/rooms/<room>/traffic`; the end-to-end test
(`node scripts/e2e-remote.mjs`) uses it to check that the relay only ever sees
ciphertext. Never enable it on a deployed relay.

## Hosting your own

The relay is a plain Worker (`wrangler.toml`). Deploying it needs your own
Cloudflare account (`wrangler deploy`) and, ideally, a route on your domain;
the desktop then uses `wss://<your host>`.
