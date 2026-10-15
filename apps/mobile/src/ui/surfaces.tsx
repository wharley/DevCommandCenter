/**
 * The quiet surfaces of the new layout: grouped lists instead of bordered
 * cards (a border is a signal, kept for what needs you), chips for choices,
 * fades where content runs under floating chrome, the project's dither, and
 * the header of a sheet.
 */
import Feather from "@expo/vector-icons/Feather";
import type { NativeStackNavigationOptions } from "@react-navigation/native-stack";
import { Children, Fragment, useId, type ReactNode } from "react";
import { Pressable, StyleSheet, Text, View, type StyleProp, type ViewStyle } from "react-native";
import Svg, { Defs, LinearGradient, Mask, Pattern, Rect, Stop } from "react-native-svg";
import { alpha, font, makeStyles, radius, useTheme, type Palette } from "../theme";
import { haptic } from "./haptics";

/** Rows on one rounded surface, with inset hairlines between them. */
export function GroupedList({
	children,
	inset = 14,
	style,
}: {
	children: ReactNode;
	/** Where the separator starts: past the leading glyph when rows have one. */
	inset?: number;
	style?: StyleProp<ViewStyle>;
}) {
	const styles = useStyles();
	const rows = Children.toArray(children).filter(Boolean);
	return (
		<View style={[styles.group, style]}>
			{rows.map((row, index) => (
				<Fragment key={index}>
					{index > 0 ? <View style={[styles.separator, { marginLeft: inset }]} /> : null}
					{row}
				</Fragment>
			))}
		</View>
	);
}

/** One line of a grouped list: leading glyph, title over a mono detail, trailing meta. */
export function Row({
	leading,
	title,
	detail,
	trailing,
	onPress,
	chevron = false,
	accessibilityLabel,
}: {
	leading?: ReactNode;
	title: string;
	/** Mono line under the title: branch, path, live step. */
	detail?: ReactNode;
	trailing?: ReactNode;
	onPress?: () => void;
	chevron?: boolean;
	accessibilityLabel?: string;
}) {
	const { color } = useTheme();
	const styles = useStyles();
	return (
		<Pressable
			onPress={onPress}
			disabled={!onPress}
			accessibilityRole={onPress ? "button" : undefined}
			accessibilityLabel={accessibilityLabel ?? title}
			style={({ pressed }) => [styles.row, pressed && { backgroundColor: color.elevated }]}
		>
			{leading}
			<View style={styles.rowBody}>
				<Text style={styles.rowTitle} numberOfLines={1}>
					{title}
				</Text>
				{typeof detail === "string" ? (
					<Text style={styles.rowDetail} numberOfLines={1}>
						{detail}
					</Text>
				) : (
					detail
				)}
			</View>
			{trailing}
			{chevron ? <Feather name="chevron-right" size={16} color={color.faint} /> : null}
		</Pressable>
	);
}

/** A choice in a row of options: agent, access, worktree. */
export function Chip({
	label,
	icon,
	leading,
	selected = false,
	menu = false,
	onPress,
}: {
	label: string;
	icon?: keyof typeof Feather.glyphMap;
	leading?: ReactNode;
	/** On: green tint (a toggle that is set). */
	selected?: boolean;
	/** Opens a picker: trailing chevron. */
	menu?: boolean;
	onPress?: () => void;
}) {
	const { color } = useTheme();
	const styles = useStyles();
	const tint = selected ? color.accent : color.fg;
	return (
		<Pressable
			onPress={
				onPress
					? () => {
							haptic("select");
							onPress();
						}
					: undefined
			}
			accessibilityRole="button"
			accessibilityState={{ selected }}
			style={({ pressed }) => [styles.chip, selected && styles.chipOn, pressed && { opacity: 0.75 }]}
		>
			{leading}
			{icon ? <Feather name={icon} size={13} color={selected ? color.accent : color.mute} /> : null}
			<Text style={[styles.chipText, { color: tint }]} numberOfLines={1}>
				{label}
			</Text>
			{selected && !menu ? <Feather name="check" size={13} color={color.accent} /> : null}
			{menu ? <Feather name="chevron-down" size={12} color={color.mute} /> : null}
		</Pressable>
	);
}

/**
 * Background fading into the edge where content scrolls under floating
 * chrome. Place it absolutely, above the content and below the chrome.
 */
export function EdgeFade({ edge, height }: { edge: "top" | "bottom"; height: number }) {
	const { color } = useTheme();
	const id = useId().replace(/:/g, "");
	// Top: solid behind the chrome, clearing toward the content; bottom mirrors it.
	const [from, to] = edge === "top" ? [{ at: "0.45", o: 1 }, { at: "1", o: 0 }] : [{ at: "0", o: 0 }, { at: "0.6", o: 1 }];
	return (
		<View pointerEvents="none" style={[StyleSheet.absoluteFill, edge === "top" ? { bottom: undefined } : { top: undefined }, { height }]}>
			<Svg width="100%" height="100%">
				<Defs>
					<LinearGradient id={id} x1="0" y1="0" x2="0" y2="1">
						<Stop offset={from.at} stopColor={color.bg} stopOpacity={from.o} />
						<Stop offset={to.at} stopColor={color.bg} stopOpacity={to.o} />
					</LinearGradient>
				</Defs>
				<Rect width="100%" height="100%" fill={`url(#${id})`} />
			</Svg>
		</View>
	);
}

/**
 * The project's dither: a pixel checker in the project color, densest at
 * the top and dissolving downward. Sits behind the new-task hero.
 */
export function DitherHero({ tint, height = 460 }: { tint: string; height?: number }) {
	const { scheme } = useTheme();
	const id = useId().replace(/:/g, "");
	const strength = scheme === "dark" ? 0.34 : 0.22;
	return (
		<View pointerEvents="none" style={[StyleSheet.absoluteFill, { bottom: undefined, height }]}>
			<Svg width="100%" height="100%">
				<Defs>
					<Pattern id={`${id}fine`} width="6" height="6" patternUnits="userSpaceOnUse">
						<Rect x="0" y="0" width="3" height="3" fill={tint} />
						<Rect x="3" y="3" width="3" height="3" fill={tint} />
					</Pattern>
					<Pattern id={`${id}coarse`} x="3" y="3" width="12" height="12" patternUnits="userSpaceOnUse">
						<Rect x="0" y="0" width="6" height="6" fill={tint} />
						<Rect x="6" y="6" width="6" height="6" fill={tint} />
					</Pattern>
					<LinearGradient id={`${id}fade`} x1="0" y1="0" x2="0" y2="1">
						<Stop offset="0" stopColor="#fff" stopOpacity={1} />
						<Stop offset="0.35" stopColor="#fff" stopOpacity={0.6} />
						<Stop offset="0.92" stopColor="#fff" stopOpacity={0} />
					</LinearGradient>
					<LinearGradient id={`${id}fadeShort`} x1="0" y1="0" x2="0" y2="1">
						<Stop offset="0" stopColor="#fff" stopOpacity={1} />
						<Stop offset="0.6" stopColor="#fff" stopOpacity={0} />
					</LinearGradient>
					<Mask id={`${id}mask`}>
						<Rect width="100%" height="100%" fill={`url(#${id}fade)`} />
					</Mask>
					<Mask id={`${id}maskShort`}>
						<Rect width="100%" height="100%" fill={`url(#${id}fadeShort)`} />
					</Mask>
				</Defs>
				<Rect width="100%" height="100%" fill={`url(#${id}fine)`} mask={`url(#${id}mask)`} opacity={strength} />
				<Rect width="100%" height="100%" fill={`url(#${id}coarse)`} mask={`url(#${id}maskShort)`} opacity={strength * 0.65} />
			</Svg>
		</View>
	);
}

/** Title and close button atop a sheet (the system draws the grabber). */
export function SheetHeader({ title, detail, onClose }: { title: string; detail?: ReactNode; onClose: () => void }) {
	const { color } = useTheme();
	const styles = useStyles();
	return (
		<View style={styles.sheetHeader}>
			<View style={{ flex: 1, minWidth: 0, gap: 4 }}>
				<Text style={styles.sheetTitle} accessibilityRole="header">
					{title}
				</Text>
				{detail}
			</View>
			<Pressable
				onPress={onClose}
				hitSlop={8}
				accessibilityRole="button"
				accessibilityLabel="Fechar"
				style={({ pressed }) => [styles.sheetClose, pressed && { opacity: 0.7 }]}
			>
				<Feather name="x" size={15} color={color.mute} />
			</Pressable>
		</View>
	);
}

/**
 * Route options for a sheet with two heights (half, full), as the system
 * draws it: liquid glass on iOS 26, a bottom sheet on Android.
 */
export function sheetOptions(color: Palette): NativeStackNavigationOptions {
	return {
		presentation: "formSheet",
		sheetAllowedDetents: [0.5, 1],
		sheetGrabberVisible: true,
		sheetCornerRadius: radius.sheet,
		contentStyle: { backgroundColor: color.bg },
	};
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		group: { borderRadius: radius.lg, backgroundColor: color.panel, overflow: "hidden" },
		separator: { height: StyleSheet.hairlineWidth, backgroundColor: color.border },
		row: { flexDirection: "row", alignItems: "center", gap: 12, paddingHorizontal: 14, paddingVertical: 12, minHeight: 56 },
		rowBody: { flex: 1, minWidth: 0, gap: 2 },
		rowTitle: { color: color.fg, fontSize: 15, fontWeight: "500", letterSpacing: -0.2 },
		rowDetail: { color: color.mute, fontSize: 12, fontFamily: font.mono },
		chip: {
			flexDirection: "row",
			alignItems: "center",
			gap: 6,
			height: 34,
			paddingHorizontal: 12,
			borderRadius: 999,
			backgroundColor: color.chip,
		},
		chipOn: { backgroundColor: alpha(color.accent, 0.12) },
		chipText: { fontSize: 13, fontWeight: "500" },
		sheetHeader: { flexDirection: "row", alignItems: "flex-start", gap: 12, paddingHorizontal: 20, paddingTop: 22, paddingBottom: 14 },
		sheetTitle: { color: color.fg, fontSize: 22, fontWeight: "700", letterSpacing: -0.4 },
		sheetClose: {
			width: 32,
			height: 32,
			borderRadius: 999,
			alignItems: "center",
			justifyContent: "center",
			backgroundColor: color.chip,
		},
	}),
);
