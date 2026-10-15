/**
 * Touch feedback with one vocabulary: `tap` for chrome, `select` for a pick,
 * `success` when an answer or a send went through, `warning` before
 * something that stops or refuses.
 */
import * as Haptics from "expo-haptics";
import { Platform } from "react-native";

export type HapticKind = "tap" | "select" | "success" | "warning";

export function haptic(kind: HapticKind): void {
	if (Platform.OS === "web") return;
	const done =
		kind === "tap"
			? Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light)
			: kind === "select"
				? Haptics.selectionAsync()
				: Haptics.notificationAsync(
						kind === "success" ? Haptics.NotificationFeedbackType.Success : Haptics.NotificationFeedbackType.Warning,
					);
	// Feedback is a nicety: a device without the engine stays silent.
	done.catch(() => {});
}
