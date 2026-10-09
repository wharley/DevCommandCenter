/**
 * This phone's identity and the desktop it is paired with, in the Keychain
 * (iOS) / Keystore (Android) through `secureStorage.ts`. The X25519 private
 * key never leaves the device; the desktop pins its public half on approval.
 */
import {
	fingerprint,
	fromBase64Url,
	generateKeyPair,
	keyPairFromPrivate,
	toBase64Url,
	type DesktopTarget,
	type KeyPair,
	type PairingOffer,
} from "@dcc/remote-protocol";
import * as Device from "expo-device";
import { Platform } from "react-native";
import { deleteItem, getItem, setItem } from "./secureStorage";

const DEVICE_KEY = "dcc.device.key.v1";
const PAIRING_KEY = "dcc.pairing.v1";
const MODE_KEY = "dcc.mode.v1";

export type Pairing = {
	relay: string;
	room: string;
	desktopPublicKey: string;
	deviceId: string;
	pairedAt: string;
};

export async function loadDeviceKey(): Promise<KeyPair> {
	const stored = await getItem(DEVICE_KEY);
	if (stored) return keyPairFromPrivate(fromBase64Url(stored));
	const keyPair = generateKeyPair();
	await setItem(DEVICE_KEY, toBase64Url(keyPair.privateKey));
	return keyPair;
}

export function deviceName(): string {
	return Device.deviceName?.trim() || Device.modelName || (Platform.OS === "ios" ? "iPhone" : "Android");
}

export function devicePlatform(): string {
	return Platform.OS;
}

export async function loadPairing(): Promise<Pairing | null> {
	const raw = await getItem(PAIRING_KEY);
	if (!raw) return null;
	try {
		const value = JSON.parse(raw) as Partial<Pairing>;
		if (value.relay && value.room && value.desktopPublicKey && value.deviceId) return value as Pairing;
	} catch {
		// unreadable: treat as unpaired
	}
	return null;
}

export async function savePairing(offer: PairingOffer, deviceId: string): Promise<Pairing> {
	const pairing: Pairing = {
		relay: offer.relay,
		room: offer.room,
		desktopPublicKey: toBase64Url(offer.desktopPublicKey),
		deviceId,
		pairedAt: new Date().toISOString(),
	};
	await setItem(PAIRING_KEY, JSON.stringify(pairing));
	return pairing;
}

/** Forgets the desktop. The device key stays, so pairing again shows the same fingerprint. */
export async function clearPairing(): Promise<void> {
	await deleteItem(PAIRING_KEY);
}

export function pairingTarget(pairing: Pairing): DesktopTarget {
	return {
		relay: pairing.relay,
		room: pairing.room,
		desktopPublicKey: fromBase64Url(pairing.desktopPublicKey),
	};
}

export function desktopFingerprint(pairing: Pairing): string {
	return fingerprint(fromBase64Url(pairing.desktopPublicKey));
}

export async function loadDemoMode(): Promise<boolean> {
	return (await getItem(MODE_KEY)) === "demo";
}

export async function saveDemoMode(on: boolean): Promise<void> {
	if (on) await setItem(MODE_KEY, "demo");
	else await deleteItem(MODE_KEY);
}
