/**
 * Dev-only gallery of the new building blocks, presented as the sheet they
 * will use, so glass, blur, haptics and both schemes can be checked on a
 * device before the screens adopt them.
 */
import { DitherHero, Chip, EdgeFade, GroupedList, Row, SheetHeader } from "@/ui/surfaces";
import { Glass, GlassButton, GlassPill } from "@/ui/glass";
import { haptic } from "@/ui/haptics";
import { AgentMascot, ProjectGlyph } from "@/ui/sprites";
import { Button, Mono, SectionLabel, StateDot } from "@/ui/primitives";
import { makeStyles, useTheme } from "@/theme";
import Feather from "@expo/vector-icons/Feather";
import { router } from "expo-router";
import { useState } from "react";
import { ScrollView, StyleSheet, Text, View } from "react-native";

export default function UiGallery() {
	const { color, scheme } = useTheme();
	const styles = useStyles();
	const [worktree, setWorktree] = useState(true);
	return (
		<View style={styles.root}>
			<SheetHeader
				title="Componentes"
				detail={<Mono style={styles.detail}>tema {scheme} · só em desenvolvimento</Mono>}
				onClose={() => router.back()}
			/>
			<ScrollView contentContainerStyle={styles.content}>
				<View style={styles.stage}>
					<DitherHero tint="#2e4fd8" height={220} />
					<View style={styles.stageBar}>
						<GlassButton icon="chevron-left" accessibilityLabel="Voltar" onPress={() => {}} />
						<GlassPill onPress={() => {}} accessibilityLabel="Trocar projeto">
							<ProjectGlyph seed="/Users/demo/DevCommandCenter" size={22} active />
							<Text style={styles.pillText}>DevCommandCenter</Text>
							<Feather name="chevron-down" size={14} color={color.mute} />
						</GlassPill>
						<GlassButton icon="settings" accessibilityLabel="Ajustes" tint={color.mute} onPress={() => {}} />
					</View>
					<View style={styles.stageFoot}>
						<Glass style={styles.composer} interactive>
							<Feather name="plus" size={18} color={color.fg} />
							<Text style={styles.placeholder}>Pedir algo em DevCommandCenter…</Text>
							<Feather name="mic" size={18} color={color.mute} />
						</Glass>
					</View>
				</View>

				<View>
					<SectionLabel tone="live">Lista agrupada</SectionLabel>
					<GroupedList inset={54}>
						<Row
							leading={<AgentMascot provider="claude" state="working" size={28} />}
							title="Ponto de atividade na sidebar"
							detail={<Mono style={{ color: color.accent, fontSize: 12 }}>editando TaskRow.tsx…</Mono>}
							trailing={<Mono style={styles.meta}>agora</Mono>}
							onPress={() => {}}
						/>
						<Row
							leading={<StateDot state="idle" size={10} />}
							title="Lista da sidebar não volta ao rolar"
							detail="fix/sidebar-scroll"
							trailing={<Mono style={styles.meta}>1 h</Mono>}
							onPress={() => {}}
							chevron
						/>
					</GroupedList>
				</View>

				<View>
					<SectionLabel>Chips</SectionLabel>
					<View style={styles.chips}>
						<Chip label="Claude · Opus 5.5" leading={<AgentMascot provider="claude" size={18} />} menu onPress={() => {}} />
						<Chip label="Pedir aprovação" icon="shield" menu onPress={() => {}} />
						<Chip label="Novo worktree" icon="git-branch" selected={worktree} onPress={() => setWorktree((v) => !v)} />
					</View>
				</View>

				<View>
					<SectionLabel tone="wait">Haptics</SectionLabel>
					<View style={styles.chips}>
						<Button label="Tap" variant="ghost" onPress={() => haptic("tap")} />
						<Button label="Select" variant="ghost" onPress={() => haptic("select")} />
						<Button label="Success" onPress={() => haptic("success")} />
						<Button label="Warning" variant="wait" onPress={() => haptic("warning")} />
					</View>
				</View>
				<View style={{ height: 80 }} />
			</ScrollView>
			<EdgeFade edge="bottom" height={60} />
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		root: { flex: 1, backgroundColor: color.bg },
		detail: { color: color.mute, fontSize: 12 },
		content: { paddingHorizontal: 16, gap: 24 },
		stage: { height: 260, borderRadius: 24, overflow: "hidden", backgroundColor: color.panel },
		stageBar: { flexDirection: "row", alignItems: "center", justifyContent: "space-between", padding: 12 },
		stageFoot: { position: "absolute", left: 12, right: 12, bottom: 12 },
		composer: { flexDirection: "row", alignItems: "center", gap: 12, height: 52, paddingHorizontal: 16 },
		placeholder: { flex: 1, color: color.mute, fontSize: 15 },
		pillText: { color: color.fg, fontSize: 15, fontWeight: "600" },
		meta: { color: color.mute, fontSize: 12 },
		chips: { flexDirection: "row", flexWrap: "wrap", gap: 8 },
	}),
);
