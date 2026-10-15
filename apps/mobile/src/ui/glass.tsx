/**
 * Floating chrome: the controls that sit over content (header buttons, the
 * scope chip, the docked composer). Liquid glass where iOS has it, a system
 * blur on older iOS, and a translucent fill on Android, where a live blur
 * behind arbitrary content costs more than it shows.
 */
import Feather from "@expo/vector-icons/Feather";
import { BlurView } from "expo-blur";
import { GlassView, isLiquidGlassAvailable } from "expo-glass-effect";
import type { ReactNode } from "react";
import { Platform, Pressable, StyleSheet, View, type StyleProp, type ViewStyle } from "react-native";
import { makeStyles, useTheme } from "../theme";
import { haptic } from "./haptics";

const liquid = Platform.OS === "ios" && isLiquidGlassAvailable();

export function Glass({
	children,
	style,
	radius = 999,
	interactive = false,
}: {
	children?: ReactNode;
	style?: StyleProp<ViewStyle>;
	radius?: number;
	/** The glass reacts to touch (iOS 26). */
	interactive?: boolean;
}) {
	const { color, scheme } = useTheme();
	const styles = useStyles();
	if (liquid) {
		return (
			<GlassView glassEffectStyle="regular" isInteractive={interactive} colorScheme={scheme} style={[{ borderRadius: radius }, style]}>
				{children}
			</GlassView>
		);
	}
	if (Platform.OS === "ios") {
		return (
			<View style={[styles.shadow, { borderRadius: radius }, style]}>
				<BlurView
					tint={scheme === "dark" ? "systemChromeMaterialDark" : "systemChromeMaterialLight"}
					intensity={80}
					style={[StyleSheet.absoluteFill, styles.edge, { borderRadius: radius }]}
				/>
				{children}
			</View>
		);
	}
	return (
		<View style={[styles.shadow, styles.edge, { borderRadius: radius, backgroundColor: color.glass }, style]}>{children}</View>
	);
}

/** A round 44pt control in glass: back, close, settings, delivery. */
export function GlassButton({
	icon,
	onPress,
	accessibilityLabel,
	tint,
	size = 44,
}: {
	icon: keyof typeof Feather.glyphMap;
	onPress: () => void;
	accessibilityLabel: string;
	tint?: string;
	size?: number;
}) {
	const { color } = useTheme();
	return (
		<Pressable
			onPress={() => {
				haptic("tap");
				onPress();
			}}
			hitSlop={6}
			accessibilityRole="button"
			accessibilityLabel={accessibilityLabel}
			style={({ pressed }) => ({ opacity: pressed && !liquid ? 0.7 : 1 })}
		>
			<Glass interactive style={{ width: size, height: size, alignItems: "center", justifyContent: "center" }}>
				<Feather name={icon} size={18} color={tint ?? color.fg} />
			</Glass>
		</Pressable>
	);
}

/** A pill in glass holding a row: the scope chip, the diff chip. */
export function GlassPill({
	children,
	onPress,
	accessibilityLabel,
	style,
}: {
	children: ReactNode;
	onPress?: () => void;
	accessibilityLabel?: string;
	style?: StyleProp<ViewStyle>;
}) {
	const styles = useStyles();
	const body = (
		<Glass interactive={!!onPress} style={[styles.pill, style]}>
			{children}
		</Glass>
	);
	if (!onPress) return body;
	return (
		<Pressable
			onPress={() => {
				haptic("tap");
				onPress();
			}}
			accessibilityRole="button"
			accessibilityLabel={accessibilityLabel}
			style={({ pressed }) => ({ opacity: pressed && !liquid ? 0.7 : 1 })}
		>
			{body}
		</Pressable>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		// The token carries the alpha, so one shadow works in both schemes. Android
		// draws elevation through translucent fills, so it goes without.
		shadow: Platform.OS === "ios" ? { shadowColor: color.shadow, shadowOpacity: 1, shadowRadius: 18, shadowOffset: { width: 0, height: 6 } } : {},
		edge: { borderWidth: StyleSheet.hairlineWidth, borderColor: color.glassBorder, overflow: "hidden" },
		pill: { flexDirection: "row", alignItems: "center", gap: 8, minHeight: 44, paddingHorizontal: 14 },
	}),
);
