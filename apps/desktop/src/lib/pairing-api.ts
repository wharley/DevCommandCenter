import { invoke } from "@tauri-apps/api/core";

export type PairedDevice = {
	deviceId: string;
	deviceName: string;
	userAgent: string | null;
	createdAt: string;
	lastUsedAt: string | null;
	lastIp: string | null;
	revoked: boolean;
	/**
	 * `relay`: pinned Noise key. `legacy`: paired over the retired LAN flow,
	 * always revoked.
	 */
	kind: "relay" | "legacy";
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

export type RemoteAccessStatus = {
	relayUrl: string | null;
	/** Set by DCC_RELAY_URL; the screen cannot change it. */
	relayUrlFromEnv: boolean;
	connected: boolean;
	roomId: string | null;
	desktopFingerprint: string | null;
	activeConnections: number;
};

export type RelayPairingOffer = {
	/** dcc://pair#... — the QR content; holds a one-time secret. */
	url: string;
	expiresAt: string;
	desktopFingerprint: string;
};

export type RelayApprovalRequest = {
	id: string;
	deviceName: string;
	platform: string | null;
	fingerprint: string;
	expiresAt: string;
};

export type RemoteAccessEvent =
	| { type: "approvalRequested"; approval: RelayApprovalRequest }
	| { type: "approvalResolved"; id: string; approved: boolean }
	| { type: "statusChanged"; connected: boolean };

export function remoteAccessStatus() {
	return invoke<RemoteAccessStatus>("remote_access_status");
}

export function remoteAccessSetRelay(relayUrl: string | null) {
	return invoke<RemoteAccessStatus>("remote_access_set_relay", { relayUrl });
}

export function remoteAccessCreateOffer() {
	return invoke<RelayPairingOffer>("remote_access_create_offer");
}

export function remoteAccessPendingApprovals() {
	return invoke<{ approvals: RelayApprovalRequest[] }>(
		"remote_access_pending_approvals",
	);
}

export function remoteAccessRespond(approvalId: string, approve: boolean) {
	return invoke<{ delivered: boolean }>("remote_access_respond", {
		approvalId,
		approve,
	});
}
