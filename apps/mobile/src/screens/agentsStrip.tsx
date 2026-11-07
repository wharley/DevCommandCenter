/**
 * "Agentes" on the home: each resident agent that talks, with its octopus,
 * how it is and what it last did. Tapping opens its own conversation (the
 * one started on the Mac); until it has one the card says so.
 */
import { residentNews, residentStateLabel, type ResidentAgent } from "@dcc/mobile-core";
import { useResidentAgents } from "@/backend/residentAgents";
import { alpha, makeStyles, radius, useTheme, type Palette } from "@/theme";
import { haptic } from "@/ui/haptics";
import { SectionLabel } from "@/ui/primitives";
import { ResidentMascot } from "@/ui/ResidentMascot";
import { router } from "expo-router";
import { Pressable, ScrollView, StyleSheet, Text, View } from "react-native";

function stateColor(agent: ResidentAgent, color: Palette): string {
	switch (agent.state) {
		case "needsYou":
			return color.wait;
		case "working":
			return color.accent;
		case "watching":
			return color.fg;
		case "asleep":
			return color.faint;
	}
}

function AgentCard({ agent, now }: { agent: ResidentAgent; now: number }) {
	const { color } = useTheme();
	const styles = useStyles();
	const session = agent.conversationSessionId;
	const state = residentStateLabel(agent.state);
	const news = residentNews(agent, now);
	return (
		<Pressable
			accessibilityRole="button"
			accessibilityLabel={`${agent.name}, ${state}. ${news}`}
			accessibilityState={{ disabled: !session }}
			onPress={() => {
				if (!session) {
					haptic("warning");
					return;
				}
				haptic("tap");
				router.push({ pathname: "/thread/[id]", params: { id: session } });
			}}
			style={({ pressed }) => [
				styles.card,
				agent.state === "needsYou" && styles.waiting,
				!session && styles.dim,
				pressed && session ? styles.pressed : null,
			]}
		>
			<ResidentMascot avatar={agent.avatar} state={agent.state} size={40} />
			<View style={styles.text}>
				<Text style={styles.name} numberOfLines={1}>
					{agent.name}
				</Text>
				<Text style={[styles.state, { color: stateColor(agent, color) }]} numberOfLines={1}>
					{state}
				</Text>
				<Text style={styles.news} numberOfLines={1}>
					{news}
				</Text>
			</View>
		</Pressable>
	);
}

export function AgentsStrip({ now = Date.now() }: { now?: number }) {
	const agents = useResidentAgents();
	const styles = useStyles();
	if (agents.length === 0) return null;
	return (
		<View style={styles.strip}>
			<SectionLabel>Agentes</SectionLabel>
			<ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.list}>
				{agents.map((agent) => (
					<AgentCard key={agent.id} agent={agent} now={now} />
				))}
			</ScrollView>
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		strip: { marginBottom: 26 },
		list: { gap: 10, paddingRight: 4 },
		card: {
			width: 196,
			flexDirection: "row",
			alignItems: "center",
			gap: 10,
			paddingHorizontal: 12,
			paddingVertical: 10,
			borderRadius: radius.lg,
			borderWidth: 1,
			borderColor: color.border,
			backgroundColor: color.panel,
		},
		waiting: { borderColor: alpha(color.waitFill, 0.45), backgroundColor: alpha(color.waitFill, 0.06) },
		dim: { opacity: 0.75 },
		pressed: { opacity: 0.7 },
		text: { flex: 1, minWidth: 0, gap: 1 },
		name: { color: color.fg, fontSize: 14, fontWeight: "600" },
		state: { fontSize: 12, fontWeight: "500" },
		news: { color: color.faint, fontSize: 11 },
	}),
);
