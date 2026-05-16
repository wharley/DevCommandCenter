import { invoke } from "@tauri-apps/api/core";

export type PairingChallenge = {
	nonce: string;
	pin: string;
	expiresAt: string;
};

export type PairedDevice = {
	deviceId: string;
	deviceName: string;
	userAgent: string | null;
	createdAt: string;
	lastUsedAt: string | null;
	lastIp: string | null;
	revoked: boolean;
};

export function pairInit() {
	return invoke<PairingChallenge>("pair_init");
}

export function pairListDevices(includeRevoked = false) {
	return invoke<{ devices: PairedDevice[] }>("pair_list_devices", {
		includeRevoked,
	});
}

export function pairRevokeDevice(deviceId: string) {
	return invoke<{ revoked: boolean }>("pair_revoke_device", { deviceId });
}
