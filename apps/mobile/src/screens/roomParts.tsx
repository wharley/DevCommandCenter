/**
 * The team room's pieces (`app/room.tsx`): what the person said, with who
 * took it; an agent's answer with its mascot, its receipt and "Abrir
 * conversa"; a message one agent passed to another as one line; and the
 * team above the field, each agent with its mascot, to address with `@`.
 */
import { handoffHeading, roomReceiptLabel, type ResidentAgent, type RoomLine } from "@dcc/mobile-core";
import { alpha, makeStyles, radius } from "@/theme";
import { haptic } from "@/ui/haptics";
import { Mono } from "@/ui/primitives";
import { ResidentMascot } from "@/ui/ResidentMascot";
import { RichText } from "@/ui/RichText";
import Feather from "@expo/vector-icons/Feather";
import { Pressable, ScrollView, StyleSheet, Text, View } from "react-native";

export function PersonLine({ line, agent }: { line: RoomLine; agent: ResidentAgent | undefined }) {
	const styles = useStyles();
	const passed = handoffHeading(line.text);
	if (passed) {
		return (
			<View style={styles.passed} accessible accessibilityLabel={passed}>
				<Feather name="corner-down-right" size={13} style={styles.passedIcon} />
				<Text style={styles.passedText} numberOfLines={2}>
					{passed.replace(/^↪ /, "")}
				</Text>
			</View>
		);
	}
	return (
		<View style={styles.personWrap}>
			<View style={styles.personBubble}>
				<Text style={styles.personText} selectable>
					{line.text}
				</Text>
			</View>
			{agent ? <Mono style={styles.routed}>→ {agent.name}</Mono> : null}
		</View>
	);
}

export function AgentLine({
	line,
	agent,
	onOpen,
}: {
	line: RoomLine;
	agent: ResidentAgent;
	onOpen: () => void;
}) {
	const styles = useStyles();
	const receipt = roomReceiptLabel(line.receipt);
	return (
		<View style={styles.agentWrap}>
			<View style={styles.agentHead}>
				<ResidentMascot avatar={agent.avatar} state="watching" size={26} />
				<Text style={styles.agentName} numberOfLines={1}>
					{agent.name}
				</Text>
			</View>
			<RichText text={line.text} />
			<View style={styles.receipt}>
				{receipt ? <Mono style={styles.receiptText}>{receipt}</Mono> : null}
				<Pressable
					onPress={() => {
						haptic("tap");
						onOpen();
					}}
					accessibilityRole="button"
					accessibilityLabel={`Abrir a conversa com ${agent.name}`}
					hitSlop={8}
				>
					<Text style={styles.open}>Abrir conversa</Text>
				</Pressable>
			</View>
		</View>
	);
}

/** "Revisor está trabalhando…" while an agent works in its conversation. */
export function BusyLine({ agent }: { agent: ResidentAgent }) {
	const styles = useStyles();
	const waiting = agent.state === "needsYou";
	return (
		<View style={styles.busy}>
			<ResidentMascot avatar={agent.avatar} state={agent.state} size={22} />
			<Text style={[styles.busyText, waiting && styles.waitText]} numberOfLines={1}>
				{waiting ? `${agent.name} precisa de você na conversa dele` : `${agent.name} está trabalhando…`}
			</Text>
		</View>
	);
}

/** The team above the field: a tap addresses that agent. */
export function TeamChips({
	agents,
	onPick,
}: {
	agents: ResidentAgent[];
	onPick: (agent: ResidentAgent) => void;
}) {
	const styles = useStyles();
	if (agents.length === 0) return null;
	return (
		<ScrollView
			horizontal
			showsHorizontalScrollIndicator={false}
			keyboardShouldPersistTaps="always"
			contentContainerStyle={styles.chips}
		>
			{agents.map((agent) => (
				<Pressable
					key={agent.id}
					onPress={() => {
						haptic("tap");
						onPick(agent);
					}}
					accessibilityRole="button"
					accessibilityLabel={`Chamar ${agent.name}`}
					style={({ pressed }) => [styles.chip, pressed && styles.pressed]}
				>
					<ResidentMascot avatar={agent.avatar} state={agent.state} size={20} />
					<Text style={styles.chipText} numberOfLines={1}>
						@{agent.name}
					</Text>
				</Pressable>
			))}
		</ScrollView>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		personWrap: { alignItems: "flex-end", gap: 4 },
		personBubble: {
			maxWidth: "86%",
			backgroundColor: color.bubble,
			borderRadius: radius.lg,
			paddingHorizontal: 14,
			paddingVertical: 10,
		},
		personText: { color: color.fg, fontSize: 15, lineHeight: 21 },
		routed: { fontSize: 11, color: color.faint },
		passed: { flexDirection: "row", alignItems: "center", gap: 6, alignSelf: "center", maxWidth: "92%" },
		passedIcon: { color: color.mute },
		passedText: { color: color.mute, fontSize: 12, flexShrink: 1 },
		agentWrap: { gap: 8 },
		agentHead: { flexDirection: "row", alignItems: "center", gap: 8 },
		agentName: { color: color.fg, fontSize: 13, fontWeight: "600", flexShrink: 1 },
		receipt: { flexDirection: "row", alignItems: "center", gap: 12 },
		receiptText: { fontSize: 11, color: color.faint },
		open: { fontSize: 12, color: color.accent, fontWeight: "500" },
		busy: { flexDirection: "row", alignItems: "center", gap: 8 },
		busyText: { color: color.mute, fontSize: 12, flexShrink: 1 },
		waitText: { color: color.wait },
		chips: { gap: 6, paddingHorizontal: 4 },
		chip: {
			flexDirection: "row",
			alignItems: "center",
			gap: 6,
			height: 32,
			paddingLeft: 6,
			paddingRight: 10,
			borderRadius: 16,
			borderWidth: 1,
			borderColor: color.border,
			backgroundColor: alpha(color.panel, 0.9),
		},
		chipText: { color: color.fg, fontSize: 13, fontWeight: "500", maxWidth: 140 },
		pressed: { opacity: 0.7 },
	}),
);
