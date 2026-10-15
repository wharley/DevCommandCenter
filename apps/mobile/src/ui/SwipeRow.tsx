/**
 * A row that slides left to reveal one action (stop, archive). Built on
 * React Native's own responder so it ships without a new native build; a
 * mostly-vertical drag stays with the list. Screen readers reach the same
 * action through the row's long-press menu.
 */
import Feather from "@expo/vector-icons/Feather";
import { useRef, type ReactNode } from "react";
import { Animated, PanResponder, Pressable, StyleSheet, Text, View } from "react-native";
import { makeStyles } from "../theme";
import { haptic } from "./haptics";

const ACTION_WIDTH = 96;

export function SwipeRow({
	children,
	action,
}: {
	children: ReactNode;
	/** Null: the row does not slide. */
	action: { label: string; icon: keyof typeof Feather.glyphMap; tint: string; ink?: string; onPress: () => void } | null;
}) {
	const styles = useStyles();
	const x = useRef(new Animated.Value(0)).current;
	const open = useRef(false);

	const settle = (to: number) => {
		open.current = to !== 0;
		Animated.spring(x, { toValue: to, useNativeDriver: true, bounciness: 0, speed: 18 }).start();
	};

	const responder = useRef(
		PanResponder.create({
			// Claim only clear horizontal drags; vertical ones scroll the list.
			onMoveShouldSetPanResponder: (_, g) => Math.abs(g.dx) > 12 && Math.abs(g.dx) > Math.abs(g.dy) * 1.5,
			onPanResponderTerminationRequest: () => false,
			onPanResponderMove: (_, g) => {
				const base = open.current ? -ACTION_WIDTH : 0;
				x.setValue(Math.min(0, Math.max(-ACTION_WIDTH * 1.4, base + g.dx)));
			},
			onPanResponderRelease: (_, g) => {
				const base = open.current ? -ACTION_WIDTH : 0;
				const reveal = base + g.dx < -ACTION_WIDTH / 2 || g.vx < -0.4;
				if (reveal && !open.current) haptic("select");
				settle(reveal ? -ACTION_WIDTH : 0);
			},
			onPanResponderTerminate: () => settle(0),
		}),
	).current;

	if (!action) return <>{children}</>;
	return (
		<View style={styles.wrap}>
			<Pressable
				onPress={() => {
					settle(0);
					action.onPress();
				}}
				accessibilityRole="button"
				accessibilityLabel={action.label}
				style={[styles.action, { backgroundColor: action.tint }]}
			>
				<Feather name={action.icon} size={18} color={action.ink ?? "#fff"} />
				<Text style={[styles.actionText, { color: action.ink ?? "#fff" }]}>{action.label}</Text>
			</Pressable>
			<Animated.View
				style={[styles.front, { transform: [{ translateX: x }] }]}
				{...responder.panHandlers}
			>
				{children}
			</Animated.View>
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		wrap: { overflow: "hidden" },
		action: {
			position: "absolute",
			top: 0,
			right: 0,
			bottom: 0,
			width: ACTION_WIDTH,
			alignItems: "center",
			justifyContent: "center",
			gap: 4,
		},
		actionText: { color: "#fff", fontSize: 12, fontWeight: "600" },
		front: { backgroundColor: color.panel },
	}),
);
