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

export type AuditEntry = {
	id: number;
	event: string;
	deviceId: string | null;
	ip: string | null;
	userAgent: string | null;
	detailsJson: string | null;
	createdAt: string;
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

export function pairAuditLog(limit = 100) {
	return invoke<{ entries: AuditEntry[] }>("pair_audit_log", { limit });
}

export function pairPurgeExpired() {
	return invoke<{ purged: number }>("pair_purge_expired");
}

export type LanEndpoint = {
	ip: string | null;
	port: number;
	url: string | null;
};

export function pairGetLanUrl() {
	return invoke<LanEndpoint>("pair_get_lan_url");
}

export type Reachability = "loopback" | "lan" | "private-network" | "public";
export type EndpointStatus = "available" | "unavailable" | "unknown";

export type AdvertisedEndpoint = {
	id: string;
	label: string;
	provider: string;
	url: string;
	reachability: Reachability;
	status: EndpointStatus;
	description: string;
};

export function pairGetEndpoints() {
	return invoke<{ endpoints: AdvertisedEndpoint[] }>("pair_get_endpoints");
}
