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
	tmuxAvailable: boolean | null;
	remoteVersion: string | null;
	remoteProtocolVersion: string | null;
	protocolCompatible: boolean | null;
};

export const REMOTE_ENV_STORAGE_KEY = "dcc.remote.environments.v1";
export const ACTIVE_REMOTE_ENV_STORAGE_KEY = "dcc.remote.environments.active.v1";
export const REMOTE_ENVIRONMENTS_CHANGED_EVENT = "dcc:remote-environments-changed";

let cachedRemoteEnvironmentsRaw: string | null | undefined;
let cachedRemoteEnvironments: SavedRemoteEnvironment[] = [];
let cachedActiveRemoteEnvironmentIdRaw: string | null | undefined;
let cachedActiveRemoteEnvironmentId: string | null = null;
let cachedActiveRemoteEnvironmentKey: string | null | undefined;
let cachedActiveRemoteEnvironment: SavedRemoteEnvironment | null = null;

function invalidateRemoteEnvironmentCache() {
	cachedRemoteEnvironmentsRaw = undefined;
	cachedRemoteEnvironments = [];
	cachedActiveRemoteEnvironmentIdRaw = undefined;
	cachedActiveRemoteEnvironmentId = null;
	cachedActiveRemoteEnvironmentKey = undefined;
	cachedActiveRemoteEnvironment = null;
}

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
		if (raw === cachedRemoteEnvironmentsRaw) {
			return cachedRemoteEnvironments;
		}

		cachedRemoteEnvironmentsRaw = raw;
		if (!raw) {
			cachedRemoteEnvironments = [];
			cachedActiveRemoteEnvironmentKey = undefined;
			cachedActiveRemoteEnvironment = null;
			return [];
		}
		const parsed = JSON.parse(raw);
		if (!Array.isArray(parsed)) {
			cachedRemoteEnvironments = [];
			cachedActiveRemoteEnvironmentKey = undefined;
			cachedActiveRemoteEnvironment = null;
			return [];
		}
		cachedRemoteEnvironments = parsed
			.map((value) => normalizeRemoteEnvironment(value))
			.filter((value): value is SavedRemoteEnvironment => value !== null);
		cachedActiveRemoteEnvironmentKey = undefined;
		cachedActiveRemoteEnvironment = null;
		return cachedRemoteEnvironments;
	} catch {
		invalidateRemoteEnvironmentCache();
		return [];
	}
}

export function writeRemoteEnvironments(next: SavedRemoteEnvironment[]) {
	if (typeof window === "undefined") {
		return;
	}
	const raw = JSON.stringify(next);
	window.localStorage.setItem(REMOTE_ENV_STORAGE_KEY, raw);
	cachedRemoteEnvironmentsRaw = raw;
	cachedRemoteEnvironments = next;
	cachedActiveRemoteEnvironmentKey = undefined;
	cachedActiveRemoteEnvironment = null;
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
		tmuxAvailable:
			typeof record.tmuxAvailable === "boolean" ? record.tmuxAvailable : null,
		remoteVersion: typeof record.remoteVersion === "string" ? record.remoteVersion : null,
		remoteProtocolVersion:
			typeof record.remoteProtocolVersion === "string"
				? record.remoteProtocolVersion
				: null,
		protocolCompatible:
			typeof record.protocolCompatible === "boolean" ? record.protocolCompatible : null,
	};
}

export function readActiveRemoteEnvironmentId(): string | null {
	if (typeof window === "undefined") {
		return null;
	}
	const raw = window.localStorage.getItem(ACTIVE_REMOTE_ENV_STORAGE_KEY);
	if (raw === cachedActiveRemoteEnvironmentIdRaw) {
		return cachedActiveRemoteEnvironmentId;
	}

	cachedActiveRemoteEnvironmentIdRaw = raw;
	const value = raw?.trim();
	cachedActiveRemoteEnvironmentId = value ? value : null;
	cachedActiveRemoteEnvironmentKey = undefined;
	cachedActiveRemoteEnvironment = null;
	return cachedActiveRemoteEnvironmentId;
}

export function writeActiveRemoteEnvironmentId(environmentId: string | null) {
	if (typeof window === "undefined") {
		return;
	}
	const normalizedEnvironmentId = environmentId?.trim() ? environmentId.trim() : null;
	if (normalizedEnvironmentId) {
		window.localStorage.setItem(ACTIVE_REMOTE_ENV_STORAGE_KEY, normalizedEnvironmentId);
	} else {
		window.localStorage.removeItem(ACTIVE_REMOTE_ENV_STORAGE_KEY);
	}
	cachedActiveRemoteEnvironmentIdRaw = normalizedEnvironmentId;
	cachedActiveRemoteEnvironmentId = normalizedEnvironmentId;
	cachedActiveRemoteEnvironmentKey = undefined;
	cachedActiveRemoteEnvironment = null;
	emitRemoteEnvironmentsChanged();
}

export function subscribeRemoteEnvironmentStore(onStoreChange: () => void) {
	if (typeof window === "undefined") {
		return () => {};
	}
	const handleChange = () => onStoreChange();
	const handleStorage = (event: StorageEvent) => {
		if (
			event.key === null ||
			event.key === REMOTE_ENV_STORAGE_KEY ||
			event.key === ACTIVE_REMOTE_ENV_STORAGE_KEY
		) {
			invalidateRemoteEnvironmentCache();
			onStoreChange();
		}
	};
	window.addEventListener(REMOTE_ENVIRONMENTS_CHANGED_EVENT, handleChange);
	window.addEventListener("storage", handleStorage);
	return () => {
		window.removeEventListener(REMOTE_ENVIRONMENTS_CHANGED_EVENT, handleChange);
		window.removeEventListener("storage", handleStorage);
	};
}

export function getActiveRemoteEnvironment(): SavedRemoteEnvironment | null {
	const activeId = readActiveRemoteEnvironmentId();
	if (!activeId) {
		return null;
	}

	const cacheKey = `${cachedRemoteEnvironmentsRaw ?? ""}:${activeId}`;
	if (cacheKey === cachedActiveRemoteEnvironmentKey) {
		return cachedActiveRemoteEnvironment;
	}

	cachedActiveRemoteEnvironmentKey = cacheKey;
	cachedActiveRemoteEnvironment =
		readRemoteEnvironments().find((environment) => environment.id === activeId) ?? null;
	return cachedActiveRemoteEnvironment;
}
