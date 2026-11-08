/**
 * The team room on the phone, as on the Mac: one conversation with the
 * resident agents. The person writes once; with `@Nome` they pick who
 * answers, without it the desktop's rule does (the Reviewer's job, who
 * answered last, the Reviewer). Each answer comes in that agent's own
 * conversation and shows here with its mascot and receipt; "Abrir conversa"
 * opens the whole of it. `?to=Nome` opens with "@Nome " in the field.
 */
import { applyFileMention, findActiveFileMention, matchingAgents, roomReasonLabel, type ResidentAgent, type RoomLine } from "@dcc/mobile-core";
import { useResidentAgents } from "@/backend/residentAgents";
import { useBackend } from "@/backend/session";
import { sendError, useTeamRoom } from "@/backend/teamRoom";
import { AgentLine, BusyLine, PersonLine, TeamChips } from "@/screens/roomParts";
import { makeStyles, useTheme } from "@/theme";
import { ConnectionBanner } from "@/ui/ConnectionBanner";
import { Glass, GlassButton } from "@/ui/glass";
import { haptic } from "@/ui/haptics";
import { Loading, Mono } from "@/ui/primitives";
import { EdgeFade } from "@/ui/surfaces";
import Feather from "@expo/vector-icons/Feather";
import { router, useLocalSearchParams } from "expo-router";
import { useEffect, useMemo, useRef, useState } from "react";
import { ActivityIndicator, FlatList, Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import { KeyboardAvoidingView, useKeyboardState } from "react-native-keyboard-controller";
import { useSafeAreaInsets } from "react-native-safe-area-context";

export default function TeamRoomScreen() {
	const backend = useBackend();
	const { color } = useTheme();
	const styles = useStyles();
	const insets = useSafeAreaInsets();
	const { to } = useLocalSearchParams<{ to?: string }>();
	const agents = useResidentAgents();
	const room = useTeamRoom(backend);
	const [text, setText] = useState(to ? `@${to} ` : "");
	const [selection, setSelection] = useState({ start: text.length, end: text.length });
	const [forced, setForced] = useState<{ start: number; end: number } | undefined>(undefined);
	const [sending, setSending] = useState(false);
	const [notice, setNotice] = useState<{ text: string; error: boolean } | null>(null);
	const [dockHeight, setDockHeight] = useState(120);
	const list = useRef<FlatList<RoomLine>>(null);
	const keyboardOpen = useKeyboardState((keyboard) => keyboard.isVisible);

	const byId = useMemo(() => new Map(agents.map((agent) => [agent.id, agent])), [agents]);
	const mention = findActiveFileMention(text, selection.end);
	const team = mention ? matchingAgents(agents, mention.query) : agents;
	const busy = agents.filter((agent) => agent.state === "working" || agent.state === "needsYou");

	useEffect(() => {
		if (room.lines.length > 0) requestAnimationFrame(() => list.current?.scrollToEnd({ animated: false }));
	}, [room.lines.length]);

	const address = (agent: ResidentAgent) => {
		if (mention) {
			const next = applyFileMention(text, mention, agent.name);
			setText(next.text);
			setForced({ start: next.cursor, end: next.cursor });
			return;
		}
		const rest = text.replace(/^@\S+\s*/, "");
		const next = `@${agent.name} ${rest}`;
		setText(next);
		setForced({ start: next.length, end: next.length });
	};

	const send = async () => {
		const message = text.trim();
		if (!message || sending) return;
		setSending(true);
		setNotice(null);
		try {
			const sent = await room.send(message);
			haptic("success");
			setText("");
			setNotice({ text: `Foi para ${sent.agentName} · ${roomReasonLabel(sent.reason)}`, error: false });
		} catch (error) {
			haptic("warning");
			setNotice({ text: sendError(error), error: true });
		} finally {
			setSending(false);
		}
	};

	const goBack = () => (router.canGoBack() ? router.back() : router.replace("/"));
	const headerSpace = insets.top + 6 + 48 + 6;
	const dockBottom = keyboardOpen ? 8 : Math.max(insets.bottom, 8);
	const canSend = text.trim().length > 0 && !sending;

	const renderItem = ({ item }: { item: RoomLine }) => {
		const agent = byId.get(item.agentId);
		if (item.speaker === "person") return <PersonLine line={item} agent={agent} />;
		if (!agent) return null;
		return (
			<AgentLine
				line={item}
				agent={agent}
				onOpen={() => router.push({ pathname: "/thread/[id]", params: { id: item.sessionId } })}
			/>
		);
	};

	return (
		<KeyboardAvoidingView style={styles.root} behavior="padding">
			<View style={styles.frame}>
				{room.loading && room.lines.length === 0 ? (
					<View style={{ flex: 1, paddingTop: headerSpace }}>
						<Loading />
					</View>
				) : (
					<FlatList
						ref={list}
						data={room.lines}
						keyExtractor={(line) => line.key}
						renderItem={renderItem}
						contentContainerStyle={[styles.listContent, { paddingTop: headerSpace, paddingBottom: dockHeight + dockBottom + 20 }]}
						ItemSeparatorComponent={() => <View style={{ height: 14 }} />}
						keyboardShouldPersistTaps="handled"
						onContentSizeChange={() => list.current?.scrollToEnd({ animated: false })}
						ListHeaderComponent={
							<View style={{ marginBottom: 8 }}>
								<ConnectionBanner backend={backend} />
							</View>
						}
						ListEmptyComponent={
							<Text style={styles.empty}>
								Ninguém falou ainda. Pergunte o que precisa de você hoje ou peça para o @Revisor olhar um PR.
							</Text>
						}
						ListFooterComponent={
							busy.length > 0 ? (
								<View style={styles.busyList}>
									{busy.map((agent) => (
										<BusyLine key={agent.id} agent={agent} />
									))}
								</View>
							) : null
						}
					/>
				)}

				<EdgeFade edge="top" height={headerSpace + 16} solid={(headerSpace - 10) / (headerSpace + 16)} />
				<EdgeFade edge="bottom" height={dockHeight + dockBottom + 28} solid={0.45} />

				<View style={[styles.header, { top: insets.top + 6 }]}>
					<GlassButton icon="chevron-left" accessibilityLabel="Voltar" onPress={goBack} />
					<View style={styles.titleBlock}>
						<Text style={styles.title} accessibilityRole="header">
							Sala da equipe
						</Text>
						<Mono style={styles.subtitle} numberOfLines={1}>
							com @ você escolhe quem responde
						</Mono>
					</View>
					<View style={{ width: 44 }} />
				</View>

				<View style={[styles.dock, { bottom: dockBottom }]} onLayout={(event) => setDockHeight(event.nativeEvent.layout.height)}>
					<TeamChips agents={team} onPick={address} />
					{notice ? (
						<Pressable onPress={() => setNotice(null)}>
							<Text style={[styles.notice, notice.error && styles.error]}>{notice.text}</Text>
						</Pressable>
					) : null}
					<Glass radius={26} style={styles.capsule}>
						<View style={styles.composerRow}>
							<TextInput
								value={text}
								onChangeText={setText}
								selection={forced}
								onSelectionChange={(event) => {
									setSelection(event.nativeEvent.selection);
									setForced(undefined);
								}}
								placeholder="Escreva para a equipe · @ para chamar alguém"
								placeholderTextColor={color.faint}
								style={styles.input}
								multiline
								autoCorrect={false}
								autoFocus={!!to}
								editable={!sending}
								accessibilityLabel="Mensagem para a equipe"
							/>
							<Pressable
								onPress={() => {
									haptic("tap");
									void send();
								}}
								disabled={!canSend}
								style={[styles.sendButton, { opacity: canSend ? 1 : 0.4 }]}
								accessibilityRole="button"
								accessibilityLabel="Enviar"
							>
								{sending ? (
									<ActivityIndicator size="small" color={color.accentInk} />
								) : (
									<Feather name="arrow-up" size={18} color={color.accentInk} />
								)}
							</Pressable>
						</View>
					</Glass>
				</View>
			</View>
		</KeyboardAvoidingView>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		root: { flex: 1, backgroundColor: color.bg },
		frame: { flex: 1 },
		header: { position: "absolute", left: 16, right: 16, flexDirection: "row", alignItems: "center", gap: 10 },
		titleBlock: { flex: 1, minWidth: 0, alignItems: "center" },
		title: { color: color.fg, fontSize: 16, fontWeight: "600", letterSpacing: -0.2 },
		subtitle: { fontSize: 11, color: color.mute, marginTop: 3 },
		listContent: { paddingHorizontal: 16 },
		empty: { color: color.mute, fontSize: 14, lineHeight: 20, marginTop: 24 },
		busyList: { gap: 8, marginTop: 14 },
		dock: { position: "absolute", left: 12, right: 12, gap: 8 },
		notice: { color: color.mute, fontSize: 12, paddingHorizontal: 8 },
		error: { color: color.danger },
		capsule: { paddingHorizontal: 8, paddingVertical: 8 },
		composerRow: { flexDirection: "row", alignItems: "flex-end", gap: 6 },
		input: {
			flex: 1,
			maxHeight: 140,
			minHeight: 40,
			color: color.fg,
			paddingHorizontal: 6,
			paddingTop: 10,
			paddingBottom: 10,
			fontSize: 16,
		},
		sendButton: {
			width: 40,
			height: 40,
			borderRadius: 20,
			backgroundColor: color.accent,
			alignItems: "center",
			justifyContent: "center",
		},
	}),
);
