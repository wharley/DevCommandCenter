/**
 * Which backend the app runs on — the paired desktop, the demo, or none yet —
 * and the lifecycle around it: restore on launch, reconnect when the app
 * comes back to the foreground, push token and presence while paired, forget
 * on unpair.
 */
import { fingerprint, RemoteClient } from "@dcc/remote-protocol";
import { createContext, use, useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { AppState } from "react-native";
import { DemoBackend } from "./demo";
import {
	clearPairing,
	deviceName,
	devicePlatform,
	loadDemoMode,
	loadDeviceKey,
	loadPairing,
	pairingTarget,
	saveDemoMode,
	type Pairing,
} from "./identity";
import { appVisibility, nativePushPlatform, pushPreferences } from "../notifications/native";
import { PushRegistration, reportPresence } from "../notifications/registration";
import { RemoteBackend, type ChannelClient } from "./remote";
import type { Backend, ConnectionState } from "./types";

export type AppSession =
	| { status: "loading" }
	| { status: "unpaired" }
	| {
			status: "remote";
			backend: RemoteBackend;
			pairing: Pairing;
			deviceFingerprint: string;
			push: PushRegistration;
	  }
	| { status: "demo"; backend: DemoBackend };

type SessionApi = {
	session: AppSession;
	startDemo(): Promise<void>;
	/** Called by the pairing screen with the channel that was just approved. */
	completePairing(pairing: Pairing, client: RemoteClient): Promise<void>;
	/** Leaves the demo or forgets the desktop, back to the pairing screen. */
	signOut(): Promise<void>;
};

const SessionContext = createContext<SessionApi | null>(null);

async function remoteSession(pairing: Pairing, initial?: ChannelClient): Promise<AppSession> {
	const keyPair = await loadDeviceKey();
	const device = { keyPair, name: deviceName(), platform: devicePlatform() };
	const backend = new RemoteBackend({
		initial,
		connect: () => RemoteClient.connect(pairingTarget(pairing), device),
	});
	const push = new PushRegistration(backend, nativePushPlatform, pushPreferences);
	return { status: "remote", backend, pairing, deviceFingerprint: fingerprint(keyPair.publicKey), push };
}

export function SessionProvider({ children }: { children: ReactNode }) {
	const [session, setSession] = useState<AppSession>({ status: "loading" });

	useEffect(() => {
		let cancelled = false;
		void (async () => {
			const pairing = await loadPairing().catch(() => null);
			const next: AppSession = pairing
				? await remoteSession(pairing)
				: (await loadDemoMode().catch(() => false))
					? { status: "demo", backend: new DemoBackend() }
					: { status: "unpaired" };
			if (cancelled) {
				if (next.status === "remote" || next.status === "demo") next.backend.dispose();
				return;
			}
			setSession(next);
		})();
		return () => {
			cancelled = true;
		};
	}, []);

	// Dispose the previous backend whenever it is replaced.
	useEffect(() => {
		if (session.status !== "remote" && session.status !== "demo") return;
		const backend = session.backend;
		return () => backend.dispose();
	}, [session]);

	useEffect(() => {
		if (session.status !== "remote") return;
		const backend = session.backend;
		const subscription = AppState.addEventListener("change", (state) => {
			if (state === "active") void backend.foreground();
		});
		return () => subscription.remove();
	}, [session]);

	const startDemo = useCallback(async () => {
		await saveDemoMode(true);
		setSession({ status: "demo", backend: new DemoBackend() });
	}, []);

	const completePairing = useCallback(async (pairing: Pairing, client: RemoteClient) => {
		await saveDemoMode(false);
		setSession(await remoteSession(pairing, client));
	}, []);

	// Push token renewed on every connection; presence while paired.
	useEffect(() => {
		if (session.status !== "remote") return;
		const stopPush = session.push.start();
		const stopPresence = reportPresence(session.backend, appVisibility);
		return () => {
			stopPush();
			stopPresence();
		};
	}, [session]);

	const signOut = useCallback(async () => {
		// Best effort: the desktop stops pushing to a phone that forgot it.
		if (session.status === "remote") {
			await Promise.race([session.push.forgetOnDesktop(), new Promise((resolve) => setTimeout(resolve, 3_000))]);
		}
		await saveDemoMode(false);
		await clearPairing();
		setSession({ status: "unpaired" });
	}, [session]);

	const value = useMemo(
		() => ({ session, startDemo, completePairing, signOut }),
		[session, startDemo, completePairing, signOut],
	);
	return <SessionContext value={value}>{children}</SessionContext>;
}

export function useSession(): SessionApi {
	const value = use(SessionContext);
	if (!value) throw new Error("useSession outside SessionProvider");
	return value;
}

/** The active backend; screens behind the session guard always have one. */
export function useBackend(): Backend {
	const { session } = useSession();
	if (session.status !== "remote" && session.status !== "demo") {
		throw new Error("no backend: the app is not paired");
	}
	return session.backend;
}

export function useConnectionState(backend: Backend): ConnectionState {
	const [state, setState] = useState(() => backend.state());
	useEffect(() => {
		setState(backend.state());
		return backend.onState(setState);
	}, [backend]);
	return state;
}
