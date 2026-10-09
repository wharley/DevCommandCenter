/** The web build (demo and tests only) has no push notifications. */
import { AppState } from "react-native";
import type { PreferenceStore, PushPlatform, VisibilitySource } from "./registration";

export function easProjectId(): string | null {
	return null;
}

export const nativePushPlatform: PushPlatform = {
	os: "web",
	getPermission: async () => ({ status: "denied", canAskAgain: false }),
	requestPermission: async () => ({ status: "denied", canAskAgain: false }),
	prepare: async () => {},
	getToken: async () => null,
	openSettings: async () => {},
};

export const pushPreferences: PreferenceStore = {
	load: async () => "off",
	save: async () => {},
};

export const appVisibility: VisibilitySource = {
	current: () => AppState.currentState,
	onChange(listener) {
		const subscription = AppState.addEventListener("change", listener);
		return () => subscription.remove();
	},
};

export function configureNotificationDisplay() {}

export function onNotificationTap(): () => void {
	return () => {};
}
