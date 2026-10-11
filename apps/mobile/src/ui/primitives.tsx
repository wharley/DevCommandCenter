/**
 * Mission-control primitives: monospace for identity/labels, a state-coded
 * node per agent, quiet panels so the one live thing is loud.
 */
import { useEffect, useRef, type ReactNode } from "react";
import {
	ActivityIndicator,
	Animated,
	Pressable,
	StyleSheet,
	Text,
	View,
	type StyleProp,
	type TextProps,
	type TextStyle,
	type ViewStyle,
} from "react-native";
import { alpha, color, font, radius } from "../theme";

export type AgentState = "live" | "wait" | "fail" | "idle";

const STATE_COLOR: Record<AgentState, string> = {
	live: color.accent,
	wait: color.wait,
	fail: color.danger,
	idle: color.faint,
};

/** A status node. Filled + breathing when live; hollow when idle. */
export function StateDot({ state, size = 8 }: { state: AgentState; size?: number }) {
	const halo = useRef(new Animated.Value(0)).current;
	useEffect(() => {
		if (state !== "live") return;
		const loop = Animated.loop(
			Animated.sequence([
				Animated.timing(halo, { toValue: 1, duration: 900, useNativeDriver: true }),
				Animated.timing(halo, { toValue: 0, duration: 900, useNativeDriver: true }),
			]),
		);
		loop.start();
		return () => loop.stop();
	}, [state, halo]);
	const tint = STATE_COLOR[state];
	return (
		<View style={{ width: size, height: size }}>
			{state === "live" ? (
				<Animated.View
					style={[
						StyleSheet.absoluteFill,
						{
							borderRadius: size,
							backgroundColor: alpha(tint, 0.45),
							opacity: halo.interpolate({ inputRange: [0, 1], outputRange: [0.6, 0] }),
							transform: [{ scale: halo.interpolate({ inputRange: [0, 1], outputRange: [1, 2.4] }) }],
						},
					]}
				/>
			) : null}
			<View
				style={{
					width: size,
					height: size,
					borderRadius: size,
					backgroundColor: state === "idle" ? "transparent" : tint,
					borderWidth: state === "idle" ? 1.5 : 0,
					borderColor: tint,
				}}
			/>
		</View>
	);
}

export function Mono({ style, children, ...rest }: TextProps & { style?: StyleProp<TextStyle>; children: ReactNode }) {
	return (
		<Text style={[styles.mono, style]} {...rest}>
			{children}
		</Text>
	);
}

/** Section eyebrow: mono label + optional count. */
export function SectionLabel({
	children,
	count,
	tone = "mute",
}: {
	children: ReactNode;
	count?: number;
	tone?: "mute" | "wait" | "live";
}) {
	const tint = tone === "wait" ? color.wait : tone === "live" ? color.accent : color.mute;
	return (
		<View style={styles.sectionLabel}>
			<Mono style={[styles.eyebrow, { color: tint }]}>{children}</Mono>
			{count !== undefined && count > 0 ? (
				<View
					style={[
						styles.count,
						{ backgroundColor: tone === "mute" ? color.elevated : tint },
					]}
				>
					<Mono
						style={[
							styles.countText,
							{ color: tone === "wait" ? color.waitInk : tone === "live" ? color.accentInk : color.mute },
						]}
					>
						{count}
					</Mono>
				</View>
			) : null}
		</View>
	);
}

/** The wordmark, with the live node when the desktop answers. */
export function Wordmark({ online }: { online?: boolean }) {
	return (
		<View style={styles.wordmark}>
			<StateDot state={online ? "live" : "idle"} />
			<Mono style={styles.wordmarkText}>DCC</Mono>
			<Mono style={styles.wordmarkSub}>central de comando</Mono>
		</View>
	);
}

export function Panel({ children, style, tone }: { children: ReactNode; style?: StyleProp<ViewStyle>; tone?: "wait" | "danger" }) {
	const border = tone === "wait" ? alpha(color.wait, 0.4) : tone === "danger" ? alpha(color.danger, 0.35) : color.border;
	const bg = tone === "wait" ? alpha(color.wait, 0.06) : tone === "danger" ? alpha(color.danger, 0.06) : color.panel;
	return <View style={[styles.panel, { borderColor: border, backgroundColor: bg }, style]}>{children}</View>;
}

export function Button({
	label,
	onPress,
	variant = "primary",
	busy,
	disabled,
	style,
	accessibilityLabel,
}: {
	label: string;
	onPress: () => void;
	variant?: "primary" | "wait" | "ghost" | "danger";
	busy?: boolean;
	disabled?: boolean;
	style?: StyleProp<ViewStyle>;
	accessibilityLabel?: string;
}) {
	const palette = {
		primary: { bg: color.accent, fg: color.accentInk, border: color.accent },
		wait: { bg: color.wait, fg: color.waitInk, border: color.wait },
		ghost: { bg: color.bg, fg: color.fg, border: color.border },
		danger: { bg: color.bg, fg: color.danger, border: alpha(color.danger, 0.5) },
	}[variant];
	const off = disabled || busy;
	return (
		<Pressable
			accessibilityRole="button"
			accessibilityLabel={accessibilityLabel ?? label}
			accessibilityState={{ disabled: !!off, busy: !!busy }}
			onPress={off ? undefined : onPress}
			style={({ pressed }) => [
				styles.button,
				{ backgroundColor: palette.bg, borderColor: palette.border, opacity: off ? 0.5 : pressed ? 0.8 : 1 },
				style,
			]}
		>
			{busy ? <ActivityIndicator size="small" color={palette.fg} /> : <Text style={[styles.buttonText, { color: palette.fg }]}>{label}</Text>}
		</Pressable>
	);
}

export function EmptyState({ title, children }: { title: string; children?: ReactNode }) {
	return (
		<View style={styles.empty}>
			<Text style={styles.emptyTitle}>{title}</Text>
			{children ? <Text style={styles.emptyBody}>{children}</Text> : null}
		</View>
	);
}

export function Loading() {
	return (
		<View style={styles.loading}>
			<ActivityIndicator color={color.mute} />
		</View>
	);
}

export const styles = StyleSheet.create({
	mono: { fontFamily: font.mono, color: color.fg },
	sectionLabel: { flexDirection: "row", alignItems: "center", gap: 8, marginBottom: 10, paddingHorizontal: 2 },
	eyebrow: { fontSize: 10, fontWeight: "600", letterSpacing: 1.8, textTransform: "uppercase" },
	count: { minWidth: 17, height: 17, borderRadius: 9, paddingHorizontal: 4, alignItems: "center", justifyContent: "center" },
	countText: { fontSize: 10, fontWeight: "700" },
	wordmark: { flexDirection: "row", alignItems: "center", gap: 8 },
	wordmarkText: { fontSize: 15, fontWeight: "700", letterSpacing: 2 },
	wordmarkSub: { fontSize: 10, color: color.faint, letterSpacing: 1.5, textTransform: "uppercase" },
	panel: { borderWidth: 1, borderRadius: radius.lg, padding: 14 },
	button: {
		minHeight: 44,
		borderRadius: radius.md,
		borderWidth: 1,
		paddingHorizontal: 14,
		alignItems: "center",
		justifyContent: "center",
	},
	buttonText: { fontSize: 14, fontWeight: "600" },
	empty: { alignItems: "center", paddingVertical: 48, paddingHorizontal: 24, gap: 6 },
	emptyTitle: { color: color.fg, fontSize: 15, fontWeight: "600" },
	emptyBody: { color: color.mute, fontSize: 13, textAlign: "center", lineHeight: 19 },
	loading: { paddingVertical: 40, alignItems: "center" },
});
