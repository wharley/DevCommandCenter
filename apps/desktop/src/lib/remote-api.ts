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
	checkedAt: string;
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
