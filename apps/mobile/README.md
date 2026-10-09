# DCC mobile (Expo)

The phone app for Dev Command Center. It reaches the desktop from any network
through the end-to-end encrypted relay (`packages/remote-protocol`,
`apps/relay`); the relay only forwards ciphertext. No accounts.

Expo SDK 55 (React 19.2.0, the same as the rest of the monorepo), Expo Router.
Shared, transport-agnostic logic lives in `packages/mobile-core`.

## Run it

Node 22 and Xcode with the iOS platform installed.

```bash
yarn install
cd apps/mobile
npx expo prebuild --platform ios      # generates ios/ (not committed)
(cd ios && LANG=en_US.UTF-8 pod install)
yarn ios                              # or build DCC.xcworkspace in Xcode
```

`yarn start` runs Metro on its own; `yarn web` serves the web build (demo and
testing only: on the web the device key sits in `localStorage`).

## A desktop to pair with

`scripts/desktop-fixture.mjs` starts a throwaway desktop on this machine: the
relay with `wrangler dev`, `dccd-http` on port 9877 with a temporary database,
app-data dir and HOME, and a disposable git repository as project/workspace.

```bash
cargo build --bin dccd-http
node apps/mobile/scripts/desktop-fixture.mjs --open   # from the repo root
```

It prints a `dcc://pair#…` link (`--open` opens it in the booted simulator;
otherwise `xcrun simctl openurl booted '<link>'`) and approves the phone when
it asks, after printing its fingerprint (`--manual` waits for `approve`/`deny`).
Type `agent` to start a Claude session in the workspace (uses your `claude`
login) with a prompt that needs a permission.

Without a desktop, "Explorar demonstração" on the pairing screen opens the demo
mode: fictional projects and an agent that streams, asks for permission and
answers, all inside the app.

## Tests

```bash
yarn workspace @dcc/mobile test
yarn workspace @dcc/mobile typecheck
```

`test/live.test.ts` runs against the fixture when given a fresh link:
`DCC_PAIR_LINK='dcc://pair#…' [DCC_E2E_AGENT=1] yarn workspace @dcc/mobile test test/live.test.ts`.
