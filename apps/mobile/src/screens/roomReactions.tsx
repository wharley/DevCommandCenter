/**
 * 👍 / 👎 under an agent's answer on the phone, as on the desktop: the one
 * given stays as a tinted pill at the start of the answer's footer (a tap
 * takes it back); the others are small quiet targets at its end, since a
 * phone has no hover to reveal them. A 👎 opens "Guardar como regra?": what
 * the person writes goes to the agent's memory as their note — no model
 * writes it.
 */
import { t, type RoomReaction } from "@dcc/mobile-core";
import { alpha, makeStyles, radius, useTheme } from "@/theme";
import { haptic } from "@/ui/haptics";
import { Button } from "@/ui/primitives";
import Feather from "@expo/vector-icons/Feather";
import { useEffect, useState } from "react";
import { Modal, Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import { KeyboardAvoidingView } from "react-native-keyboard-controller";
import { useSafeAreaInsets } from "react-native-safe-area-context";

/** The longest rule kept, as the desktop's field. */
const MAX_RULE_CHARS = 500;

const ICON: Record<RoomReaction, "thumbs-up" | "thumbs-down"> = { up: "thumbs-up", down: "thumbs-down" };
const SPOKEN: Record<RoomReaction, string> = { up: "Útil", down: "Não ajudou" };

/** The pill of the reaction given, at the start of the footer. */
export function GivenReaction({ reaction, onPress }: { reaction: RoomReaction; onPress: () => void }) {
	const { color } = useTheme();
	const styles = useStyles();
	const tint = reaction === "up" ? color.accent : color.wait;
	return (
		<Pressable
			onPress={() => {
				haptic("select");
				onPress();
			}}
			accessibilityRole="button"
			accessibilityState={{ selected: true }}
			accessibilityLabel={`${SPOKEN[reaction]}. Tocar tira a reação`}
			hitSlop={8}
			style={[styles.pill, { backgroundColor: alpha(tint, 0.12), borderColor: alpha(tint, 0.35) }]}
		>
			<Feather name={ICON[reaction]} size={12} color={tint} />
		</Pressable>
	);
}

/** The reactions not given, quiet at the end of the footer. */
export function ReactionTargets({ given, onReact }: { given: RoomReaction | null; onReact: (reaction: RoomReaction) => void }) {
	const { color } = useTheme();
	const styles = useStyles();
	return (
		<View style={styles.targets}>
			{(["up", "down"] as const)
				.filter((reaction) => reaction !== given)
				.map((reaction) => (
					<Pressable
						key={reaction}
						onPress={() => {
							haptic("select");
							onReact(reaction);
						}}
						accessibilityRole="button"
						accessibilityLabel={SPOKEN[reaction]}
						hitSlop={10}
						style={styles.target}
					>
						<Feather name={ICON[reaction]} size={13} color={color.faint} />
					</Pressable>
				))}
		</View>
	);
}

/** "Guardar como regra?" after a 👎: a sheet with the field, over the keyboard. */
export function RuleSheet({
	visible,
	busy,
	onKeep,
	onClose,
}: {
	visible: boolean;
	busy: boolean;
	onKeep: (body: string) => void;
	onClose: () => void;
}) {
	const { color } = useTheme();
	const styles = useStyles();
	const insets = useSafeAreaInsets();
	const [body, setBody] = useState("");
	useEffect(() => {
		if (visible) setBody("");
	}, [visible]);
	const ready = body.trim().length > 0 && !busy;
	return (
		<Modal visible={visible} transparent animationType="fade" onRequestClose={onClose}>
			<KeyboardAvoidingView style={styles.root} behavior="padding">
				<Pressable style={styles.scrim} onPress={onClose} accessibilityLabel="Fechar" />
				<View style={[styles.sheet, { paddingBottom: insets.bottom + 12 }]}>
					<View style={styles.grabber} />
					<Text style={styles.title} accessibilityRole="header">
						{t("agents.reactions.ruleAsk")}
					</Text>
					<TextInput
						value={body}
						onChangeText={setBody}
						placeholder={t("agents.reactions.rulePlaceholder")}
						placeholderTextColor={color.faint}
						style={styles.input}
						autoFocus
						multiline
						maxLength={MAX_RULE_CHARS}
						accessibilityLabel={t("agents.reactions.ruleAsk")}
					/>
					<View style={styles.actions}>
						<Button
							label={t("agents.reactions.keep")}
							style={styles.action}
							busy={busy}
							disabled={!ready}
							onPress={() => {
								haptic("success");
								onKeep(body.trim());
							}}
						/>
						<Button label={t("agents.reactions.skip")} variant="ghost" style={styles.action} disabled={busy} onPress={onClose} />
					</View>
				</View>
			</KeyboardAvoidingView>
		</Modal>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		pill: { height: 22, width: 30, borderRadius: 11, borderWidth: 1, alignItems: "center", justifyContent: "center" },
		targets: { flexDirection: "row", gap: 4, marginLeft: "auto" },
		target: { width: 26, height: 26, alignItems: "center", justifyContent: "center", borderRadius: 13 },
		root: { flex: 1, justifyContent: "flex-end" },
		scrim: { ...StyleSheet.absoluteFillObject, backgroundColor: color.scrim },
		sheet: {
			backgroundColor: color.bg,
			borderTopLeftRadius: radius.sheet,
			borderTopRightRadius: radius.sheet,
			paddingHorizontal: 16,
			gap: 10,
		},
		grabber: { alignSelf: "center", width: 38, height: 5, borderRadius: 999, backgroundColor: color.border, marginTop: 8 },
		title: { color: color.fg, fontSize: 17, fontWeight: "600", marginTop: 6 },
		input: {
			minHeight: 84,
			maxHeight: 160,
			color: color.fg,
			fontSize: 15,
			paddingHorizontal: 12,
			paddingVertical: 10,
			borderRadius: radius.md,
			borderWidth: 1,
			borderColor: color.border,
			backgroundColor: color.panel,
			textAlignVertical: "top",
		},
		actions: { flexDirection: "row", gap: 8 },
		action: { flex: 1, minHeight: 44 },
	}),
);
