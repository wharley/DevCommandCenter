import { invoke } from "@tauri-apps/api/core";

/** Keep the Mac awake: Off (default), Always (while DCC is open) or while agents work. */
export type KeepAwakeMode = "off" | "always" | "during_agents";

export type KeepAwakeReason =
	| { kind: "appOpen" }
	| { kind: "agentsWorking"; count: number }
	| { kind: "agentJustFinished" };

export type KeepAwakeStatus = {
	/** False off macOS: the setting is hidden. */
	supported: boolean;
	mode: KeepAwakeMode;
	active: { reason: KeepAwakeReason; label: string } | null;
};

/** Emitted to every window when the hold starts, changes reason or ends. */
export const KEEP_AWAKE_CHANGED_EVENT = "keep-awake-changed";

/** i18n key (and count) for why the Mac is being kept awake. */
export function keepAwakeReasonKey(reason: KeepAwakeReason): {
	key: string;
	count?: number;
} {
	switch (reason.kind) {
		case "agentsWorking":
			return { key: "settings.general.keepAwake.reasons.agentsWorking", count: reason.count };
		case "agentJustFinished":
			return { key: "settings.general.keepAwake.reasons.agentJustFinished" };
		default:
			return { key: "settings.general.keepAwake.reasons.appOpen" };
	}
}

export const readKeepAwake = () => invoke<KeepAwakeStatus>("keep_awake_status");
export const setKeepAwakeMode = (mode: KeepAwakeMode) =>
	invoke<KeepAwakeStatus>("keep_awake_set_mode", { mode });
