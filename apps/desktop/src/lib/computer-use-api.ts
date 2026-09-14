import { invoke } from "@tauri-apps/api/core";
import { openExternal } from "./shell-api";

export type ComputerUsePermission = {
	granted: boolean;
	canRequest: boolean;
};

export type ComputerUseTarget = {
	bundleId: string;
	name: string;
	pid: number;
	windowId: number;
	title: string;
	x: number;
	y: number;
	width: number;
	height: number;
};

export type ComputerUseGrant = {
	sessionId: string | null;
	armed: boolean;
	remainingMs: number;
	allowedBundleIds: string[];
};

export type ComputerUseStatus = {
	platform: string;
	supported: boolean;
	unsupportedReason: string | null;
	minimumMacosVersion: number | null;
	providerSupported: boolean;
	runtimeAttached: boolean;
	accessibility: ComputerUsePermission;
	screenRecording: ComputerUsePermission;
	grant: ComputerUseGrant;
	targets: ComputerUseTarget[];
};

export type ComputerUseArmInput = {
	sessionId: string;
	allowedBundleIds: string[];
};

export type ComputerUseGrantStatus = ComputerUseGrant;

export type ComputerUseAccessKind = "accessibility" | "screenRecording";

export type ComputerUsePendingRequest = {
	requestId: string;
	sessionId: string;
	workspaceId: string;
	providerId: string;
	reason: string;
	remainingMs: number;
};

export type ComputerUseRespondControlRequestInput = {
	requestId: string;
	allowed: boolean;
	allowedBundleIds: string[];
};

const COMPUTER_USE_METHODS = {
	status: "computer_use_status",
	arm: "computer_use_arm",
	disarm: "computer_use_disarm",
	requestAccess: "computer_use_request_access",
	listPendingRequests: "computer_use_list_pending_requests",
	respondControlRequest: "computer_use_respond_control_request",
} as const;

function isTauriRuntime(): boolean {
	return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function unsupportedDesktopStatus(): ComputerUseStatus {
	return {
		platform: "web",
		supported: false,
		unsupportedReason: "Computer Use requires the supported desktop runtime.",
		minimumMacosVersion: null,
		providerSupported: false,
		runtimeAttached: false,
		accessibility: { granted: false, canRequest: false },
		screenRecording: { granted: false, canRequest: false },
		grant: { sessionId: null, armed: false, remainingMs: 0, allowedBundleIds: [] },
		targets: [],
	};
}

export async function getComputerUseStatus(
	sessionId?: string | null,
): Promise<ComputerUseStatus> {
	if (!isTauriRuntime()) return unsupportedDesktopStatus();
	return invoke<ComputerUseStatus>(COMPUTER_USE_METHODS.status, {
		input: { sessionId: sessionId ?? null, includeTargets: true },
	});
}

export async function armComputerUse(
	input: ComputerUseArmInput,
): Promise<ComputerUseGrantStatus> {
	if (!isTauriRuntime()) {
		throw new Error("Computer Use requires the desktop runtime");
	}
	return invoke<ComputerUseGrantStatus>(COMPUTER_USE_METHODS.arm, { input });
}

export async function disarmComputerUse(
	input: Pick<ComputerUseArmInput, "sessionId">,
): Promise<ComputerUseGrantStatus> {
	if (!isTauriRuntime()) {
		throw new Error("Computer Use requires the desktop runtime");
	}
	return invoke<ComputerUseGrantStatus>(COMPUTER_USE_METHODS.disarm, { input });
}

export async function requestComputerUseAccess(
	kind: ComputerUseAccessKind,
): Promise<ComputerUseStatus> {
	if (!isTauriRuntime()) {
		throw new Error("Computer Use requires the desktop runtime");
	}
	const settingsUrl =
		kind === "accessibility"
			? "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
			: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";
	const opened = await openExternal(settingsUrl);
	if (!opened.success) {
		throw new Error("Could not open macOS Privacy settings");
	}
	return invoke<ComputerUseStatus>(COMPUTER_USE_METHODS.requestAccess, {
		input: { kind },
	});
}

export async function listComputerUsePendingRequests(): Promise<ComputerUsePendingRequest[]> {
	if (!isTauriRuntime()) return [];
	const result = await invoke<{ requests: ComputerUsePendingRequest[] }>(
		COMPUTER_USE_METHODS.listPendingRequests,
	);
	return result.requests;
}

export async function respondComputerUseControlRequest(
	input: ComputerUseRespondControlRequestInput,
): Promise<{ grant: ComputerUseGrant }> {
	if (!isTauriRuntime()) {
		throw new Error("Computer Use requires the desktop runtime");
	}
	return invoke<{ grant: ComputerUseGrant }>(
		COMPUTER_USE_METHODS.respondControlRequest,
		{ input },
	);
}
