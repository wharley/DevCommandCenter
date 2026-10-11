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

Android: the Android SDK with an emulator image, and a JDK (the Gradle 9
wrapper ran on Temurin 23 here). Prebuild with `--platform android`, then
`yarn android`, or `cd android && ./gradlew :app:assembleDebug` and
`adb install`. With Metro running, `adb reverse tcp:8081 tcp:8081` lets the
emulator load the bundle.

## A desktop to pair with

`scripts/desktop-fixture.mjs` starts a throwaway desktop on this machine: the
relay with `wrangler dev`, `dccd-http` on port 9877 with a temporary database,
app-data dir and HOME, a disposable git repository as project/workspace, and a
second project with no workspace yet (Workspaces lists it with "Nova tarefa").

```bash
cargo build --bin dccd-http
node apps/mobile/scripts/desktop-fixture.mjs --open   # from the repo root
```

It also stands in for Expo's push service (see "Push notifications").
It prints a `dcc://pair#…` link (`--open` opens it in the booted simulator;
otherwise `xcrun simctl openurl booted '<link>'`; `--android` forwards the
relay port into the running emulator with `adb reverse` and opens it there)
and approves the phone when it asks, after printing its fingerprint (`--manual` waits for `approve`/`deny`).
Type `agent` to start a Claude session in the workspace (uses your `claude`
login) with a prompt that needs a permission.

Without a desktop, "Explorar demonstração" on the pairing screen opens the demo
mode: fictional projects and an agent that streams, asks for permission and
answers, all inside the app.

## Push notifications

The desktop sends a push when a task needs the person (permission request,
question from the agent, task finished or interrupted), straight to Expo's
push service — no server of ours in between. The phone hands its Expo push
token to the desktop only over the encrypted channel
(`PUT /api/v1/remote/push-token`) and renews it on every connection (48 h
lease). The text is generic ("DCC" · "<project> precisa de você"); the payload
carries only the session id that the tap opens. Code:
`src/notifications/` here, `src-tauri/src/remote_push.rs` on the desktop.

Expo issues push tokens only to an app tied to an EAS project, so until the
steps below are done the app shows "Este build não tem o projectId do EAS" in
Settings and the desktop never sends anything. Nothing else changes in code:
the app reads `extra.eas.projectId` from `app.json` (or `Constants.easConfig`).

### Turning on real push (one time, by the account owner)

1. **Expo account and project.** Create/log into the account at expo.dev, then
   from `apps/mobile`:
   ```bash
   npx eas-cli@latest login
   npx eas-cli@latest init
   ```
   `eas init` creates the project and writes `extra.eas.projectId` (and
   `owner`) into `app.json`. Commit that change — the id is not a secret.
2. **iOS (APNs).** Needs the paid Apple Developer account. Register the
   iPhone for internal builds (`npx eas-cli device:create`, open the link on
   the phone), then either let the first `eas build` set it up (answer yes to
   "Setup Push Notifications for your project" and "Generating a new Apple Push
   Notifications service key") or run `npx eas-cli credentials` → iOS →
   Push Notifications: Manage your Apple Push Notifications Key → generate
   (or upload an existing `.p8`). EAS keeps the key; it never goes into this
   repository.
3. **Android (FCM V1).** In the Firebase console create a project with an
   Android app `com.devcommandcenter.mobile`.
   - Download `google-services.json`. It stays out of git (`.gitignore`):
     put it in `apps/mobile/` for local builds, and give it to EAS as a file
     variable for cloud builds — `app.config.js` picks whichever exists:
     ```bash
     npx eas-cli env:create --name GOOGLE_SERVICES_JSON --type file --value ./google-services.json --visibility secret --environment preview --environment production
     ```
   - Project settings → Service accounts → generate a private key (JSON, keep
     it out of git) and upload it: `npx eas-cli credentials` → Android →
     production → Google Service Account → Manage your Google Service Account
     Key for Push Notifications (FCM V1) → Set up → Upload a new service
     account key.
4. **Build for a real iPhone.** (Expo says simulators on Xcode 14+ can get
   tokens too, but the phone is the test that counts: lock screen, background.)
   ```bash
   npx eas-cli build --profile preview --platform ios   # or: npx expo run:ios --device
   ```
   `preview` (in `eas.json`) is a standalone build for internal distribution:
   it installs from the link EAS prints, on the iPhones registered with
   `device:create`, and needs no Metro server. `production` is the App Store
   build (`eas submit` sends it to TestFlight).
   Install it, pair with the desktop, accept "Avisar quando um agente precisar
   de você?". Settings → "Notificações deste aparelho" should say "Ligadas".
5. **Check it.** Put the app in the background (or lock the phone), step away
   from the desktop (the desktop app holds pushes while its window has focus
   and keyboard/mouse were used in the last 3 minutes) and ask an agent for
   something that needs a permission. The notification opens the thread.
   To test the token alone, paste it into https://expo.dev/notifications.

### Trying it in the simulator without an EAS project

The fixture below plays Expo's push service and delivers each push to the
booted simulator with `xcrun simctl push`, in the APNs payload Expo builds.
Give the debug build a stand-in token through Metro:

```bash
EXPO_PUBLIC_DCC_DEV_PUSH_TOKEN='ExponentPushToken[simulator]' npx expo start
node apps/mobile/scripts/desktop-fixture.mjs --open   # from the repo root
```

Pair, accept notifications, send the app to the background (Home), type
`agent` in the fixture: the permission request arrives as a banner, and
tapping it opens the thread. `push <sessionId>` in the fixture delivers a
sample notification without an agent.

## Dictation and attachments

The composer of a thread and the request in "Nova tarefa" have a microphone
and a paperclip.

- **Dictation** uses the phone's own recognizer (`expo-speech-recognition`:
  SFSpeechRecognizer on iOS, SpeechRecognizer on Android) in pt-BR, on the
  device when it can (iOS when the pt-BR model supports it; Android when
  "Android System Intelligence" has pt-BR installed), otherwise the system's
  regular recognizer (Apple's or Google's service). The audio never goes to the
  desktop: the recognized text lands in the field after what was typed, and
  nothing is sent until the person taps send. Microphone and speech
  permissions are asked the first time only. Logic: `src/dictation/` and
  `dictation.ts` in `packages/mobile-core`.
- **Attachments**: photos (library or camera) are resized to 2048 px on the
  longest edge and re-encoded as JPEG; other files go as they are. Each file is
  limited to 10 MB. A picked file uploads right away over the encrypted channel
  (`POST /api/v1/mobile/attachments`) and the desktop saves it where it keeps
  pasted images (its temp dir); the message then carries `@/path`, the same
  way the desktop composer serializes attachments, so the agent receives the
  image natively. The thread shows images as thumbnails, fetched with
  `GET /api/v1/mobile/sessions/:id/attachment`, which only serves paths that
  session's own prompts attached.

In the iOS Simulator, dictation uses the Mac's microphone (it may not
recognize anything without one); `xcrun simctl addmedia booted <image>` puts
a photo in the simulator's library.

## Updates without a new build (EAS Update)

The app ships with `expo-updates`: on launch it asks EAS Update
(`updates.url` in `app.json`, the project's `u.expo.dev` URL) for a newer
JavaScript bundle on its build's channel, downloads it in the background and
runs it on the next start; coming back to the foreground checks again at most
every 30 minutes, and a banner offers "Reiniciar" once a new version is
waiting. Settings shows which code the phone runs (channel, embedded build or
update id). Development builds never check.

Channels follow the build profile in `eas.json`: `preview` builds listen on
`preview`, `production` builds on `production`. To ship JavaScript-only changes
(screens, texts, logic in `packages/mobile-core`) to the phones on a channel,
from `apps/mobile`, logged into the Expo account that owns the project:

```bash
npx eas-cli@latest update --channel preview --message "o que mudou"
```

Anything native needs a new build instead (`eas build --profile preview`):
adding or upgrading a library with native code, a config plugin, permissions
or anything else in `app.json` that ends up in the iOS/Android project. The
runtime version is the project's **fingerprint** (`runtimeVersion.policy`), so
an update built from a tree whose native side differs from a build's is simply
never offered to that build — it keeps running what it has instead of crashing
— but it also means that build won't see the update until it is rebuilt.
`preview` builds bump their build number by themselves (`autoIncrement`, with
the version source kept on EAS).

Whoever can publish to this EAS project controls the code running on every
paired phone, which holds the key to the desktop. Keep publishing limited to
the project owner's Expo account (no other members with publish rights, no
`EXPO_TOKEN` in CI), and publish only from a reviewed, committed tree.

## Tests

```bash
yarn workspace @dcc/mobile test
yarn workspace @dcc/mobile typecheck
```

`test/live.test.ts` runs against the fixture when given a fresh link:
`DCC_PAIR_LINK='dcc://pair#…' [DCC_E2E_AGENT=1] yarn workspace @dcc/mobile test test/live.test.ts`.
