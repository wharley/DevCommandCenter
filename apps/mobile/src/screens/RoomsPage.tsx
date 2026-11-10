/**
 * The home's "Salas" page: Geral and each project's room as a messages
 * list — the project's pixel mascot (or `#` for Geral), the room's name,
 * what it is doing or its latest line, when, and a mark for what is new or
 * waits on the person. A tap opens the room; the team's strip sits under it.
 */
import { roomKey, roomStatus, roomTime, roomUnread, type CatalogRepository, type Room } from "@dcc/mobile-core";
import { useResidentAgents } from "@/backend/residentAgents";
import type { RoomsState } from "@/backend/rooms";
import { AgentsStrip } from "@/screens/agentsStrip";
import { hairline, makeStyles, useTheme } from "@/theme";
import { haptic } from "@/ui/haptics";
import { EmptyState, Loading, Mono, SectionLabel, StateDot } from "@/ui/primitives";
import { ResidentMascot } from "@/ui/ResidentMascot";
import { ProjectGlyph } from "@/ui/sprites";
import Feather from "@expo/vector-icons/Feather";
import { router } from "expo-router";
import { Pressable, StyleSheet, Text, View } from "react-native";

const TILE = 44;

export function openRoom(room: Pick<Room, "id" | "name">) {
	router.push(room.id ? { pathname: "/room", params: { room: room.id, name: room.name } } : "/room");
}

function RoomRow({
	room,
	repository,
	unread,
	last,
}: {
	room: Room;
	repository: CatalogRepository | undefined;
	unread: boolean;
	last: boolean;
}) {
	const { color } = useTheme();
	const styles = useStyles();
	const agents = useResidentAgents();
	const status = roomStatus(room, (id) => agents.find((agent) => agent.id === id)?.name ?? null);
	const working = agents.filter((agent) => room.working.includes(agent.id)).slice(0, 3);
	const live = room.working.length > 0 || room.running;
	const tint = status.tone === "wait" ? color.wait : status.tone === "live" ? color.accent : color.mute;
	return (
		<Pressable
			onPress={() => {
				haptic("tap");
				openRoom(room);
			}}
			accessibilityRole="button"
			accessibilityLabel={`Sala ${room.name}. ${status.text}${unread ? ". Tem novidade" : ""}`}
			style={({ pressed }) => [styles.row, pressed && { backgroundColor: color.elevated }]}
		>
			{room.id === null ? (
				<View style={styles.hash}>
					<Feather name="hash" size={20} color={color.fg} />
				</View>
			) : (
				<ProjectGlyph icon={repository?.icon} color={repository?.color} seed={repository?.rootPath ?? room.id} active={live} size={TILE} />
			)}
			<View style={[styles.body, !last && styles.divider]}>
				<View style={styles.line}>
					<Text style={[styles.name, unread && styles.nameUnread]} numberOfLines={1}>
						{room.name}
					</Text>
					<Mono style={[styles.time, unread && { color: color.accent }]}>{roomTime(room.at)}</Mono>
				</View>
				<View style={styles.line}>
					{status.tone !== "idle" ? <StateDot state={status.tone === "wait" ? "wait" : "live"} size={7} /> : null}
					<Text style={[styles.status, { color: tint }, status.tone !== "idle" && styles.statusLoud]} numberOfLines={1}>
						{status.text}
					</Text>
					{working.length > 0 ? (
						<View style={styles.faces}>
							{working.map((agent) => (
								<ResidentMascot key={agent.id} avatar={agent.avatar} state="working" size={18} />
							))}
						</View>
					) : unread ? (
						<View style={styles.unread} />
					) : null}
				</View>
			</View>
		</Pressable>
	);
}

export function RoomsPage({ state, repositories }: { state: RoomsState; repositories: CatalogRepository[] }) {
	const styles = useStyles();
	const { rooms, seen, unsupported } = state;
	if (!rooms) return <Loading />;
	if (unsupported) {
		return (
			<View style={styles.page}>
				<EmptyState icon="hash" title="As salas chegam com o DCC novo no computador">
					Atualize o DCC no Mac para ver uma sala por projeto aqui. A Sala da equipe continua aberta.
				</EmptyState>
				<AgentsStrip />
			</View>
		);
	}
	const byProject = new Map(repositories.map((repository) => [repository.projectId, repository]));
	return (
		<View style={styles.page}>
			<View>
				<SectionLabel>Salas</SectionLabel>
				<View style={styles.list}>
					{rooms.map((room, index) => (
						<RoomRow
							key={roomKey(room.id)}
							room={room}
							repository={room.id ? byProject.get(room.id) : undefined}
							unread={seen ? roomUnread(room, seen.seen, seen.baseline) : false}
							last={index === rooms.length - 1}
						/>
					))}
				</View>
				<Text style={styles.hint}>Uma sala por projeto: crie no computador, em Agentes › Nova sala.</Text>
			</View>
			<AgentsStrip />
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		page: { gap: 26 },
		list: { borderRadius: 20, backgroundColor: color.panel, overflow: "hidden" },
		row: { flexDirection: "row", alignItems: "center", gap: 12, paddingLeft: 12, minHeight: 68 },
		hash: { width: TILE, height: TILE, borderRadius: 13, alignItems: "center", justifyContent: "center", backgroundColor: color.chip },
		body: { flex: 1, minWidth: 0, gap: 3, paddingVertical: 12, paddingRight: 14 },
		divider: { borderBottomWidth: hairline, borderBottomColor: color.border },
		line: { flexDirection: "row", alignItems: "center", gap: 6 },
		name: { flex: 1, color: color.fg, fontSize: 16, fontWeight: "500", letterSpacing: -0.2 },
		nameUnread: { fontWeight: "700" },
		time: { color: color.faint, fontSize: 12 },
		status: { flex: 1, fontSize: 14, lineHeight: 19 },
		statusLoud: { fontWeight: "500" },
		faces: { flexDirection: "row", gap: 2 },
		unread: { width: 9, height: 9, borderRadius: 5, backgroundColor: color.accent },
		hint: { color: color.faint, fontSize: 12, marginTop: 8, paddingHorizontal: 4 },
	}),
);
