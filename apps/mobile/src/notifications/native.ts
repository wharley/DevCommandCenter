/**
 * Expo's side of push notifications: permission, Android channel, the Expo
 * push token, taps and how a notification shows while the app is open.
 * `registration.ts` holds the logic; this file only adapts the platform.
 */
import Constants from "expo-constants";
import * as Notifications from "expo-notifications";
import { AppState, Linking, Platform } from "react-native";
import { getItem, setItem } from "../backend/secureStorage";
import type {
	PermissionInfo,
	PreferenceStore,
	PushPlatform,
	PushPreference,
	VisibilitySource,
} from "./registration";

const PREFERENCE_KEY = "dcc.push.v1";
/** The desktop names this channel in every push (`remote_push::ANDROID_CHANNEL`). */
const ANDROID_CHANNEL = "attention";

/**
 * The EAS project the Expo push token belongs to: `extra.eas.projectId` in
 * app.json, written by `eas init` (apps/mobile/README.md). Without it Expo
 * cannot issue a token and notifications stay off.
 */
export function easProjectId(): string | null {
	const fromEas = Constants.easConfig?.projectId;
	const fromExtra = (Constants.expoConfig?.extra as { eas?: { projectId?: unknown } } | undefined)?.eas?.projectId;
	const id = typeof fromEas === "string" && fromEas.trim() ? fromEas : fromExtra;
	return typeof id === "string" && id.trim() ? id.trim() : null;
}

/**
 * Debug builds only: a fixed token to try the whole path (desktop → fake push
 * service → `xcrun simctl push`) in the simulator without an EAS project.
 */
function devToken(): string | null {
	if (!__DEV__) return null;
	const token = process.env.EXPO_PUBLIC_DCC_DEV_PUSH_TOKEN;
	return token?.trim() ? token.trim() : null;
}

function permissionInfo(result: Notifications.NotificationPermissionsStatus): PermissionInfo {
	const status = result.granted ? "granted" : result.status === "denied" ? "denied" : "undetermined";
	return { status, canAskAgain: result.canAskAgain };
}

export const nativePushPlatform: PushPlatform = {
	os: Platform.OS === "android" ? "android" : "ios",
	getPermission: async () => permissionInfo(await Notifications.getPermissionsAsync()),
	requestPermission: async () =>
		permissionInfo(
			await Notifications.requestPermissionsAsync({
				ios: { allowAlert: true, allowSound: true, allowBadge: false },
			}),
		),
	async prepare() {
		if (Platform.OS !== "android") return;
		await Notifications.setNotificationChannelAsync(ANDROID_CHANNEL, {
			name: "Precisa de você",
			description: "Pedidos de permissão, perguntas e tarefas concluídas no seu computador.",
			importance: Notifications.AndroidImportance.HIGH,
		});
	},
	async getToken() {
		const projectId = easProjectId();
		if (!projectId) return devToken();
		const { data } = await Notifications.getExpoPushTokenAsync({ projectId });
		return data?.trim() || null;
	},
	openSettings: () => Linking.openSettings(),
};

export const pushPreferences: PreferenceStore = {
	async load() {
		const stored = await getItem(PREFERENCE_KEY);
		return stored === "on" || stored === "off" ? stored : "unset";
	},
	async save(preference: PushPreference) {
		await setItem(PREFERENCE_KEY, preference);
	},
};

export const appVisibility: VisibilitySource = {
	current: () => AppState.currentState,
	onChange(listener) {
		const subscription = AppState.addEventListener("change", listener);
		return () => subscription.remove();
	},
};

/**
 * The desktop does not push to a phone with the app open; if one arrives
 * anyway (the app just came to the front), show it quietly.
 */
export function configureNotificationDisplay() {
	Notifications.setNotificationHandler({
		handleNotification: async () => ({
			shouldShowBanner: true,
			shouldShowList: true,
			shouldPlaySound: false,
			shouldSetBadge: false,
		}),
	});
}

/**
 * Calls `open` with the data of each tapped notification: the one that
 * launched the app, then every later tap. Each tap once.
 */
export function onNotificationTap(open: (data: unknown) => void): () => void {
	const seen = new Set<string>();
	const handle = (response: Notifications.NotificationResponse | null) => {
		if (!response || response.actionIdentifier !== Notifications.DEFAULT_ACTION_IDENTIFIER) return;
		const id = response.notification.request.identifier;
		if (seen.has(id)) return;
		seen.add(id);
		open(response.notification.request.content.data);
		Notifications.clearLastNotificationResponse();
	};
	handle(Notifications.getLastNotificationResponse());
	const subscription = Notifications.addNotificationResponseReceivedListener(handle);
	return () => subscription.remove();
}
