import { invoke } from "@tauri-apps/api/core";

export type RemoteTunnelSnapshot = {
	environmentId: string;
	sshTarget: string;
	localPort: number;
	remotePort: number;
	endpoint: string;
	bearerToken: string;
	startedAt: string;
	remoteCommand: string;
	tmuxAvailable: boolean | null;
	remoteVersion: string | null;
	remoteProtocolVersion: string | null;
	protocolCompatible: boolean | null;
	status: "running" | "exited" | "error" | string;
	exitCode: number | null;
};

export type RemoteTunnelLaunchInput = {
	environmentId: string;
	sshTarget: string;
	remoteCommand?: string | null;
	localPort?: number | null;
	remotePort?: number | null;
	bearerToken?: string | null;
};

export type RemotePreflightInput = {
	sshTarget: string;
	remoteCommand?: string | null;
};

export type RemotePreflightSnapshot = {
	sshReachable: boolean;
	remoteCommandFound: boolean;
	tmuxAvailable: boolean | null;
	platformName: string | null;
	platformArch: string | null;
	binaryCompatible: boolean | null;
	errorMessage: string | null;
	checkedAt: string;
};

export type RemoteBootstrapInput = {
	sshTarget: string;
};

export type RemoteBootstrapSnapshot = {
	installedPath: string;
	remoteCommand: string;
	tmuxAvailable: boolean | null;
	binaryCompatible: boolean | null;
	checkedAt: string;
};

export type RemoteMobileAccessSnapshot = {
	environmentId: string;
	localEndpoint: string;
	lanEndpoint: string | null;
	lanEndpoints: string[];
	sharePort: number;
	bearerToken: string;
	startedAt: string;
};

export function preflightRemoteSsh(input: RemotePreflightInput) {
	return invoke<RemotePreflightSnapshot>("remote_preflight_ssh", {
		input,
	});
}

export function bootstrapRemoteSshBinary(input: RemoteBootstrapInput) {
	return invoke<RemoteBootstrapSnapshot>("remote_bootstrap_ssh_binary", {
		input,
	});
}

export function listRemoteSshTunnels() {
	return invoke<{ tunnels: RemoteTunnelSnapshot[] }>("remote_list_ssh_tunnels");
}

export function launchRemoteSshTunnel(input: RemoteTunnelLaunchInput) {
	return invoke<{ ok: boolean; tunnel: RemoteTunnelSnapshot }>("remote_launch_ssh_tunnel", {
		input,
	});
}

export function stopRemoteSshTunnel(environmentId: string) {
	return invoke<{ ok: boolean }>("remote_stop_ssh_tunnel", {
		environmentId,
	});
}

export function openRemoteMobileAccess(environmentId: string) {
	return invoke<RemoteMobileAccessSnapshot>("remote_open_mobile_access", {
		input: { environmentId },
	});
}

export function closeRemoteMobileAccess(environmentId: string) {
	return invoke<{ ok: boolean }>("remote_close_mobile_access", {
		environmentId,
	});
}
