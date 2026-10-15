/**
 * Over-the-air updates (`expo-updates`, EAS Update). The launch itself checks
 * (`checkAutomatically: ON_LOAD` in app.json) and a downloaded update applies
 * on the next start; coming back to the foreground checks again at most every
 * 30 minutes. When a new version is waiting, a banner offers to restart now.
 * JS only: anything native (a new library, a permission) needs a new build —
 * the `fingerprint` runtime version keeps an update from reaching a build it
 * doesn't fit. Off in development builds (`Updates.isEnabled` is false).
 */
import Feather from "@expo/vector-icons/Feather";
import * as Updates from "expo-updates";
import { useEffect, useRef, useState } from "react";
import { AppState, Pressable, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { makeStyles, radius, useTheme } from "./theme";

const RECHECK_MS = 30 * 60 * 1000;

async function checkAndFetch(): Promise<void> {
	try {
		const check = await Updates.checkForUpdateAsync();
		if (check.isAvailable) await Updates.fetchUpdateAsync();
	} catch {
		// Offline or no update server: the build keeps running what it has.
	}
}

export function UpdateBanner() {
	const { color } = useTheme();
	const styles = useStyles();
	const { isUpdatePending } = Updates.useUpdates();
	const [dismissed, setDismissed] = useState(false);
	const lastCheck = useRef(Date.now());
	const insets = useSafeAreaInsets();

	useEffect(() => {
		if (!Updates.isEnabled) return;
		const subscription = AppState.addEventListener("change", (state) => {
			if (state !== "active" || Date.now() - lastCheck.current < RECHECK_MS) return;
			lastCheck.current = Date.now();
			void checkAndFetch();
		});
		return () => subscription.remove();
	}, []);

	if (!Updates.isEnabled || !isUpdatePending || dismissed) return null;
	return (
		<View style={[styles.wrap, { top: insets.top + 8 }]} pointerEvents="box-none">
			<View style={styles.banner}>
				<Feather name="download" size={14} color={color.accent} />
				<Text style={styles.text}>Nova versão do app baixada.</Text>
				<Pressable onPress={() => void Updates.reloadAsync()} hitSlop={8} accessibilityRole="button">
					<Text style={styles.action}>Reiniciar</Text>
				</Pressable>
				<Pressable onPress={() => setDismissed(true)} hitSlop={8} accessibilityRole="button" accessibilityLabel="Depois">
					<Feather name="x" size={14} color={color.mute} />
				</Pressable>
			</View>
		</View>
	);
}

/** "0.1.0 · preview · a1b2c3d" for Settings: which code this phone is running. */
export function appVersionLine(version: string | null | undefined): string {
	const parts = [version ?? "?"];
	if (Updates.isEnabled) {
		if (Updates.channel) parts.push(Updates.channel);
		parts.push(Updates.isEmbeddedLaunch ? "build" : `atualização ${(Updates.updateId ?? "").slice(0, 7)}`);
	} else {
		parts.push("desenvolvimento");
	}
	return parts.join(" · ");
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		wrap: { position: "absolute", left: 12, right: 12, alignItems: "center", zIndex: 10 },
		banner: {
			flexDirection: "row",
			alignItems: "center",
			gap: 10,
			paddingHorizontal: 14,
			paddingVertical: 10,
			borderRadius: radius.md,
			borderWidth: 1,
			borderColor: color.border,
			backgroundColor: color.elevated,
		},
		text: { color: color.fg, fontSize: 13 },
		action: { color: color.accent, fontSize: 13, fontWeight: "700" },
	}),
);
