export type SavedRemoteEnvironment = {
	id: string;
	label: string;
	sshTarget: string;
	remoteCommand: string;
	localPort: number | null;
	remotePort: number;
	bearerToken: string | null;
	endpoint: string | null;
	lastStartedAt: string | null;
};

export const REMOTE_ENV_STORAGE_KEY = "dcc.remote.environments.v1";
export const ACTIVE_REMOTE_ENV_STORAGE_KEY = "dcc.remote.environments.active.v1";
export const REMOTE_ENVIRONMENTS_CHANGED_EVENT = "dcc:remote-environments-changed";

function emitRemoteEnvironmentsChanged() {
	if (typeof window === "undefined") {
		return;
	}
	window.dispatchEvent(new CustomEvent(REMOTE_ENVIRONMENTS_CHANGED_EVENT));
}

export function readRemoteEnvironments(): SavedRemoteEnvironment[] {
	if (typeof window === "undefined") {
		return [];
	}

	try {
		const raw = window.localStorage.getItem(REMOTE_ENV_STORAGE_KEY);
		if (!raw) {
			return [];
		}
		const parsed = JSON.parse(raw);
		if (!Array.isArray(parsed)) {
			return [];
		}
		return parsed
			.map((value) => normalizeRemoteEnvironment(value))
			.filter((value): value is SavedRemoteEnvironment => value !== null);
	} catch {
		return [];
	}
}

export function writeRemoteEnvironments(next: SavedRemoteEnvironment[]) {
	if (typeof window === "undefined") {
		return;
	}
	window.localStorage.setItem(REMOTE_ENV_STORAGE_KEY, JSON.stringify(next));
	emitRemoteEnvironmentsChanged();
}

export function normalizeRemoteEnvironment(value: unknown): SavedRemoteEnvironment | null {
	if (!value || typeof value !== "object") {
		return null;
	}

	const record = value as Record<string, unknown>;
	const id = typeof record.id === "string" ? record.id : "";
	const label = typeof record.label === "string" ? record.label : "";
	const sshTarget = typeof record.sshTarget === "string" ? record.sshTarget : "";
	if (!id || !label || !sshTarget) {
		return null;
	}

	return {
		id,
		label,
		sshTarget,
		remoteCommand:
			typeof record.remoteCommand === "string" && record.remoteCommand.trim()
				? record.remoteCommand
				: "dccd-http",
		localPort:
			typeof record.localPort === "number" && Number.isFinite(record.localPort)
				? record.localPort
				: null,
		remotePort:
			typeof record.remotePort === "number" && Number.isFinite(record.remotePort)
				? record.remotePort
				: 9876,
		bearerToken: typeof record.bearerToken === "string" ? record.bearerToken : null,
		endpoint: typeof record.endpoint === "string" ? record.endpoint : null,
		lastStartedAt: typeof record.lastStartedAt === "string" ? record.lastStartedAt : null,
	};
}

export function readActiveRemoteEnvironmentId(): string | null {
	if (typeof window === "undefined") {
		return null;
	}
	const value = window.localStorage.getItem(ACTIVE_REMOTE_ENV_STORAGE_KEY)?.trim();
	return value ? value : null;
}

export function writeActiveRemoteEnvironmentId(environmentId: string | null) {
	if (typeof window === "undefined") {
		return;
	}
	if (environmentId && environmentId.trim()) {
		window.localStorage.setItem(ACTIVE_REMOTE_ENV_STORAGE_KEY, environmentId);
		emitRemoteEnvironmentsChanged();
		return;
	}
	window.localStorage.removeItem(ACTIVE_REMOTE_ENV_STORAGE_KEY);
	emitRemoteEnvironmentsChanged();
}

export function subscribeRemoteEnvironmentStore(onStoreChange: () => void) {
	if (typeof window === "undefined") {
		return () => {};
	}
	const handleChange = () => onStoreChange();
	window.addEventListener(REMOTE_ENVIRONMENTS_CHANGED_EVENT, handleChange);
	return () => {
		window.removeEventListener(REMOTE_ENVIRONMENTS_CHANGED_EVENT, handleChange);
	};
}

export function getActiveRemoteEnvironment(): SavedRemoteEnvironment | null {
	const activeId = readActiveRemoteEnvironmentId();
	if (!activeId) {
		return null;
	}
	return (
		readRemoteEnvironments().find((environment) => environment.id === activeId) ?? null
	);
}
