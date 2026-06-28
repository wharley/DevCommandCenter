# Mobile Web Companion

The mobile web companion is served by `dccd-http` and can be paired with the
desktop app through a QR code plus PIN. It is intended for trusted local
networks or private network overlays such as Tailscale.

## Server Model

There are two different servers during development:

| Server | Port | Serves | Use it for |
|---|---:|---|---|
| `dccd-http` | `9876` | The HTTP API (`/api`, `/auth`) and the built SPA at `/m/` from `apps/mobile-web/dist/` | QR pairing and phone testing without Vite |
| Vite dev server | `5174` | The SPA at `/m/` with HMR, proxying `/api`, `/auth`, `/health`, and `/rpc` to `127.0.0.1:9876` | Live mobile UI development |

Important details:

- The HTTP daemon is required in both flows because it owns the API and reads the
  same application database as the desktop app.
- The desktop QR code points at `http://<host>:9876/m/`, so QR pairing expects a
  built `apps/mobile-web/dist/`.
- When using the Vite server, open the Vite URL manually and omit the `be`
  fragment parameter so the SPA uses the Vite origin and its API proxy.

## Start The HTTP Daemon

`yarn dev`, `yarn dev:desktop`, and `tauri dev` do not start the mobile HTTP
daemon. Run it separately:

```bash
cd src-tauri
cargo run --bin dccd-http
```

The daemon should log that it is listening on port `9876`. From the same machine:

```bash
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:9876/health
```

Expected response code: `200`.

Keep a single desktop instance open when pairing. The desktop app creates the
pairing nonce and PIN; `dccd-http` validates the pairing and serves the mobile
API.

## Phone Via QR

This is the recommended phone flow because the QR code opens the same daemon
that serves the API.

1. Start `dccd-http`.
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

## Phone With Vite HMR

Use this when changing the mobile UI and testing on a physical phone.

1. Start `dccd-http`.
2. Expose Vite on the network:

   ```bash
   cd apps/mobile-web
   yarn dev --host
   ```

3. In the desktop app, create a new pairing and copy the nonce plus PIN.
4. On the phone, open the Vite URL manually:

   ```text
   http://<lan-ip>:5174/m/pair#nonce=<NONCE>
   ```

5. Enter the PIN shown by the desktop app.

Do not scan the QR code for the Vite flow. The QR code points to `:9876`, not
`:5174`.

## Notebook Browser Loop

For a quick local browser loop:

```text
http://localhost:5174/m/
http://localhost:5174/m/pair#nonce=<NONCE>
```

The Vite proxy sends API calls to the local `dccd-http` daemon.

## Tailscale

Tailscale is useful when the phone and desktop are not on the same LAN, or when
you want transport encryption through the tailnet.

1. Install and log into Tailscale on both devices.
2. Get the desktop machine's Tailscale IPv4 address:

   ```bash
   tailscale ip -4
   ```

3. Start `dccd-http`.
4. Use one of the flows below.

For QR pairing:

```bash
cd apps/mobile-web
yarn build
```

Then choose the Tailscale endpoint in the desktop pairing UI and scan the QR code
from the phone while Tailscale is connected.

For Vite HMR:

```bash
cd apps/mobile-web
yarn dev --host
```

Open:

```text
http://<tailscale-ip>:5174/m/pair#nonce=<NONCE>
```

Prefer the numeric Tailscale IP for Vite. MagicDNS hostnames can be blocked by
Vite's allowed-host checks unless explicitly configured in
`apps/mobile-web/vite.config.ts`.

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
| The QR URL does not open on the phone | `dccd-http` is not reachable or `apps/mobile-web/dist/` is missing | Start `dccd-http` and run `yarn build` in `apps/mobile-web` |
| `localhost:5174/m/` does not load | The wrong path or server was opened | Use `http://localhost:5174/m/` while Vite is running |
| The phone cannot reach Vite | Vite is bound to localhost only | Run `yarn dev --host` and use the displayed network URL |
| API calls fail in the Vite flow | The URL includes `be=` and bypasses the Vite proxy | Use `/m/pair#nonce=<NONCE>` without `be=` |
| Pairing says the nonce is invalid | The pairing window expired | Create a new pairing in the desktop app |
| Port `9876` refuses connections | The HTTP daemon is not running or another process owns the port | Restart `dccd-http` and check the daemon logs |
