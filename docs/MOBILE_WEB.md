# Mobile Web Companion

The mobile web companion is served by the desktop app itself (or by the
headless `dccd-http` server when no desktop app runs) and is paired through a
QR code plus PIN. It is intended for trusted local
networks or private network overlays such as Tailscale.

## Server Model

There are two different servers during development:

| Server | Port | Serves | Use it for |
|---|---:|---|---|
| Desktop app (or `dccd-http`) | `9876` | The HTTP API (`/api`, `/auth`) and the built SPA at `/m/` from `apps/mobile-web/dist/` | QR pairing and phone testing without OJ |
| OJ dev server | `5174` | The SPA at `/m/` with HMR, proxying `/api`, `/auth`, `/health`, and `/rpc` to `127.0.0.1:9876` | Live mobile UI development |

Important details:

- The HTTP API is required in both flows. Served by the desktop app it runs on
  the app's own sessions, so the phone sees and answers (permissions,
  questions) the agents the desktop is running; tasks started from the phone
  run in the desktop and show in its sidebar.
- The desktop QR code points at `http://<host>:9876/m/`, so QR pairing expects a
  built `apps/mobile-web/dist/`.
- When using the OJ server, open the OJ URL manually and omit the `be`
  fragment parameter so the SPA uses the OJ origin and its API proxy.

## Start The HTTP API

The desktop app (`tauri dev` included) serves the API on `127.0.0.1:9876` while
it is open. Opening Settings -> Connections (mobile pairing) is the opt-in that
moves it to every interface so a phone can reach it; it stays there until the
app quits. In that mode the API accepts only paired devices (or an operator
bearer token), never the local `X-API-Key`. Remote terminals
(`/api/v1/terminals/*`) are not served by the desktop app.

Without the desktop app, run the headless server (it owns the sessions it
starts and still serves terminals):

```bash
cd src-tauri
DCC_HTTP_LAN=1 cargo run --bin dccd-http
```

`dccd-http` listens on `127.0.0.1` unless `DCC_HTTP_LAN=1` is set. Do not run it
next to the desktop app: the app stops a `dccd-http` it finds on the port at
startup. The API key is generated on first run and stored in
`~/.dcc/http-config.json` (mode `0600`); the server refuses to start with the
old public default key.

The server should be listening on port `9876`. From the same machine:

```bash
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:9876/health
```

Expected response code: `200`.

Keep a single desktop instance open: a second copy finds the port taken and
serves no API (it only logs a warning).

## Phone Via QR

This is the recommended phone flow because the QR code opens the same daemon
that serves the API.

1. Open the desktop app (or start `dccd-http`).
2. Build the mobile SPA:

   ```bash
   cd apps/mobile-web
   yarn build
   ```

   During active development you can keep the build fresh with:

   ```bash
   yarn build --watch
   ```

3. In the desktop app, open Settings -> Connections -> Pair new device.
4. Choose the LAN or Tailscale endpoint, scan the QR code on the phone, and enter
   the PIN shown by the desktop app.

The generated `apps/mobile-web/dist/` directory is intentionally ignored by Git.

## Phone With OJ HMR

Use this when changing the mobile UI and testing on a physical phone.

1. Open the desktop app (or start `dccd-http`).
2. Expose OJ on the network:

   ```bash
   cd apps/mobile-web
   yarn dev --host
   ```

3. In the desktop app, create a new pairing and copy the nonce plus PIN.
4. On the phone, open the OJ URL manually:

   ```text
   http://<lan-ip>:5174/m/pair#nonce=<NONCE>
   ```

5. Enter the PIN shown by the desktop app.

Do not scan the QR code for the OJ flow. The QR code points to `:9876`, not
`:5174`.

## Notebook Browser Loop

For a quick local browser loop:

```text
http://localhost:5174/m/
http://localhost:5174/m/pair#nonce=<NONCE>
```

The OJ proxy sends API calls to the local HTTP API on `:9876`.

## Tailscale

Tailscale is useful when the phone and desktop are not on the same LAN, or when
you want transport encryption through the tailnet.

1. Install and log into Tailscale on both devices.
2. Get the desktop machine's Tailscale IPv4 address:

   ```bash
   tailscale ip -4
   ```

3. Open the desktop app (or start `dccd-http`).
4. Use one of the flows below.

For QR pairing:

```bash
cd apps/mobile-web
yarn build
```

Then choose the Tailscale endpoint in the desktop pairing UI and scan the QR code
from the phone while Tailscale is connected.

For OJ HMR:

```bash
cd apps/mobile-web
yarn dev --host
```

Open:

```text
http://<tailscale-ip>:5174/m/pair#nonce=<NONCE>
```

Prefer the numeric Tailscale IP for OJ. If the phone cannot reach the server,
pass `--host` to the dev command and use the displayed network URL.

## Smoke Test

After pairing, verify these mobile paths:

- A blocked agent appears in the "Needs you" section and can be resolved inline.
- A running agent appears in the running section and updates its thread state.
- The thread header shows live worktree insertion/deletion counts when the agent
  edits files.
- Workspaces show diff pills, and tapping one opens the file-level diff summary.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| The QR URL does not open on the phone | The API is not reachable on the LAN or `apps/mobile-web/dist/` is missing | Open Settings -> Connections in the desktop app (LAN opt-in) and run `yarn build` in `apps/mobile-web` |
| `localhost:5174/m/` does not load | The wrong path or server was opened | Use `http://localhost:5174/m/` while OJ is running |
| The phone cannot reach OJ | OJ is bound to localhost only | Run `yarn dev --host` and use the displayed network URL |
| API calls fail in the OJ flow | The URL includes `be=` and bypasses the OJ proxy | Use `/m/pair#nonce=<NONCE>` without `be=` |
| Pairing says the nonce is invalid | The pairing window expired | Create a new pairing in the desktop app |
| Port `9876` refuses connections | The desktop app is closed, or another process (a second app copy) owns the port | Keep one desktop app open and look for `[DCC HTTP]` in its logs |
