/**
 * A room on the phone, as on the Mac: Geral, or a project's (`?room=<id>`,
 * `&name=`), one conversation with the resident agents. The person writes
 * once; with `@Nome` they pick who answers, without it the desktop's rule
 * does. Answers read as flat rows under each agent's mascot; an approval an
 * agent waits on is answered right on the field; files, photos and
 * dictation go in as in a task. `?to=Nome` opens with "@Nome " written.
 */
import {
	applyFileMention,
	findActiveFileMention,
	matchingAgents,
	roomItems,
	roomReasonLabel,
	type ResidentAgent,
	type RoomApproval,
	type RoomItem,
	type RoomQuestion,
} from "@dcc/mobile-core";
import { useResidentAgents } from "@/backend/residentAgents";
import { markRoomSeen } from "@/backend/roomsSeen";
import { useBackend } from "@/backend/session";
import { sendError, useTeamRoom } from "@/backend/teamRoom";
import { useDictation } from "@/dictation/useDictation";
import { AgentLine, ApprovalDock, DayPill, MentionList, PersonLine, WorkingLine } from "@/screens/roomParts";
import { QuestionDock } from "@/screens/roomQuestion";
import { makeStyles, useTheme } from "@/theme";
import { AttachButton, AttachmentTray, DictationStatus, MicButton, useAttachments } from "@/ui/ComposerTools";
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
	const params = useLocalSearchParams<{ to?: string; room?: string; name?: string }>();
	const roomId = params.room || null;
	const roomName = roomId ? params.name || "Projeto" : "Geral";
	const agents = useResidentAgents();
	const room = useTeamRoom(backend, roomId);
	const [text, setText] = useState(params.to ? `@${params.to} ` : "");
	const [selection, setSelection] = useState({ start: text.length, end: text.length });
	const [forced, setForced] = useState<{ start: number; end: number } | undefined>(undefined);
	const [sending, setSending] = useState(false);
	const [notice, setNotice] = useState<{ text: string; error: boolean } | null>(null);
	const [dockHeight, setDockHeight] = useState(120);
	const attachments = useAttachments(backend);
	const dictation = useDictation(text, setText);
	const input = useRef<TextInput>(null);
	const list = useRef<FlatList<RoomItem>>(null);
	const keyboardOpen = useKeyboardState((keyboard) => keyboard.isVisible);

	const byId = useMemo(() => new Map(agents.map((agent) => [agent.id, agent])), [agents]);
	const items = useMemo(() => roomItems(room.lines), [room.lines]);
	const mention = findActiveFileMention(text, selection.end);
	const matches = mention ? matchingAgents(agents, mention.query) : [];
	// Who works in this room; an older desktop only says how each agent is anywhere.
	const working = room.working
		? agents.filter((agent) => room.working!.includes(agent.id))
		: roomId
			? []
			: agents.filter((agent) => agent.state === "working");

	const lastAt = room.lines.at(-1)?.at ?? null;
	useEffect(() => {
		// Open is read: what is on screen stops being news on the home.
		if (!room.loading) markRoomSeen(roomId, lastAt);
	}, [roomId, lastAt, room.loading]);

	useEffect(() => {
		if (items.length > 0) requestAnimationFrame(() => list.current?.scrollToEnd({ animated: false }));
	}, [items.length, working.length]);

	const address = (agent: ResidentAgent) => {
		if (!mention) return;
		const next = applyFileMention(text, mention, agent.name);
		setText(next.text);
		setForced({ start: next.cursor, end: next.cursor });
		// The pick keeps the person writing.
		input.current?.focus();
	};

	/** The `@` button: starts a mention where the person writes. */
	const startMention = () => {
		haptic("tap");
		const at = selection.end;
		const glued = at > 0 && !/\s/.test(text[at - 1] ?? "");
		const insert = glued ? " @" : "@";
		const next = text.slice(0, at) + insert + text.slice(at);
		setText(next);
		setSelection({ start: at + insert.length, end: at + insert.length });
		setForced({ start: at + insert.length, end: at + insert.length });
		input.current?.focus();
	};

	const uploading = attachments.items.some((item) => item.status === "uploading");
	const hasReady = attachments.items.some((item) => item.status === "ready");
	const canSend = (text.trim().length > 0 || hasReady) && !uploading && !sending;

	const send = async () => {
		if (!canSend) return;
		if (dictation.listening) dictation.stop();
		const message = attachments.queue.prompt(text);
		if (!message) return;
		setSending(true);
		setNotice(null);
		try {
			const sent = await room.send(message);
			haptic("success");
			setText("");
			attachments.queue.clear();
			setNotice({ text: `Foi para ${sent.agentName} · ${roomReasonLabel(sent.reason)}`, error: false });
		} catch (error) {
			haptic("warning");
			setNotice({ text: sendError(error), error: true });
		} finally {
			setSending(false);
		}
	};

	const reply = (question: RoomQuestion, answers: Array<{ question: string; answer: string }>) => {
		haptic("success");
		void room.reply(question, answers).then((failed) => failed && setNotice({ text: failed, error: true }));
	};

	const answer = (approval: RoomApproval, allow: boolean) => {
		haptic(allow ? "success" : "warning");
		void room.answer(approval, allow).then((failed) => failed && setNotice({ text: failed, error: true }));
	};

	const goBack = () => (router.canGoBack() ? router.back() : router.replace("/"));
	const headerSpace = insets.top + 6 + 48 + 6;
	const dockBottom = keyboardOpen ? 8 : Math.max(insets.bottom, 8);
	const asking = room.approvals.length > 0 || room.questions.length > 0;
	const subtitle =
		asking
			? "precisa de você"
			: working.length === 1
				? `${working[0]!.name} está trabalhando`
				: working.length > 1
					? `${working.length} agentes trabalhando`
					: `${agents.length} ${agents.length === 1 ? "agente" : "agentes"} · @ escolhe quem responde`;

	const renderItem = ({ item }: { item: RoomItem }) => {
		if (item.type === "day") return <DayPill label={item.label} />;
		const agent = byId.get(item.line.agentId);
		if (item.line.speaker === "person") return <PersonLine line={item.line} agent={agent} team={agents} />;
		if (!agent) return null;
		return (
			<AgentLine
				line={item.line}
				agent={agent}
				first={item.first}
				onOpen={() => router.push({ pathname: "/thread/[id]", params: { id: item.line.sessionId } })}
				onTask={(task, where) =>
					where === "thread" && task.sessionId
						? router.push({ pathname: "/thread/[id]", params: { id: task.sessionId } })
						: router.push({ pathname: "/delivery/[id]", params: { id: task.workspaceId } })
				}
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
						data={items}
						keyExtractor={(item) => item.key}
						renderItem={renderItem}
						contentContainerStyle={[styles.listContent, { paddingTop: headerSpace, paddingBottom: dockHeight + dockBottom + 20 }]}
						ItemSeparatorComponent={({ leadingItem }) => {
							const next = items[items.indexOf(leadingItem as RoomItem) + 1];
							// Answers of one group sit close; a new speaker gets air.
							return <View style={{ height: next?.type === "line" && !next.first ? 8 : 18 }} />;
						}}
						keyboardShouldPersistTaps="handled"
						onContentSizeChange={() => list.current?.scrollToEnd({ animated: false })}
						ListHeaderComponent={
							<View style={{ marginBottom: 8 }}>
								<ConnectionBanner backend={backend} />
							</View>
						}
						ListEmptyComponent={
							<Text style={styles.empty}>
								{roomId
									? `Ninguém falou em ${roomName} ainda. Peça algo do projeto: aqui os agentes já sabem que é ele.`
									: "Ninguém falou ainda. Pergunte o que precisa de você hoje ou peça para o @Revisor olhar um PR."}
							</Text>
						}
						ListFooterComponent={
							working.length > 0 ? (
								<View style={styles.working}>
									{working.map((agent) => (
										<WorkingLine key={agent.id} agent={agent} />
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
						<Text style={styles.title} accessibilityRole="header" numberOfLines={1}>
							<Text style={styles.hash}># </Text>
							{roomName}
						</Text>
						<Mono style={[styles.subtitle, asking && { color: color.wait }, working.length > 0 && !asking && { color: color.accent }]} numberOfLines={1}>
							{subtitle}
						</Mono>
					</View>
					<View style={{ width: 44 }} />
				</View>

				<View style={[styles.dock, { bottom: dockBottom }]} onLayout={(event) => setDockHeight(event.nativeEvent.layout.height)}>
					<MentionList agents={matches} onPick={address} />
					{notice ? (
						<Pressable onPress={() => setNotice(null)}>
							<Text style={[styles.notice, notice.error && styles.error]}>{notice.text}</Text>
						</Pressable>
					) : null}
					<QuestionDock questions={room.questions} answering={room.answering} onAnswer={reply} />
					<ApprovalDock approvals={room.approvals} answering={room.answering} onAnswer={answer} />
					<Glass radius={24} style={styles.card}>
						<AttachmentTray attachments={attachments} />
						<DictationStatus dictation={dictation} />
						<TextInput
							ref={input}
							value={text}
							onChangeText={setText}
							selection={forced}
							onSelectionChange={(event) => {
								setSelection(event.nativeEvent.selection);
								setForced(undefined);
							}}
							placeholder={roomId ? `Escreva para a equipe do ${roomName}` : "Escreva para a equipe"}
							placeholderTextColor={color.faint}
							style={styles.input}
							multiline
							autoCorrect={false}
							autoFocus={!!params.to}
							editable={!sending && !dictation.listening}
							accessibilityLabel={`Mensagem para a sala ${roomName}`}
						/>
						<View style={styles.bar}>
							<AttachButton attachments={attachments} disabled={sending} />
							<Pressable onPress={startMention} disabled={sending} accessibilityRole="button" accessibilityLabel="Chamar um agente" hitSlop={4} style={styles.tool}>
								<Feather name="at-sign" size={18} color={color.mute} />
							</Pressable>
							<MicButton dictation={dictation} disabled={sending} />
							<View style={{ flex: 1 }} />
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
								{sending ? <ActivityIndicator size="small" color={color.accentInk} /> : <Feather name="arrow-up" size={18} color={color.accentInk} />}
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
		title: { color: color.fg, fontSize: 17, fontWeight: "600", letterSpacing: -0.2 },
		hash: { color: color.faint, fontWeight: "500" },
		subtitle: { fontSize: 11, color: color.mute, marginTop: 3 },
		listContent: { paddingHorizontal: 16 },
		empty: { color: color.mute, fontSize: 14, lineHeight: 20, marginTop: 24 },
		working: { gap: 10, marginTop: 18 },
		dock: { position: "absolute", left: 12, right: 12, gap: 8 },
		notice: { color: color.mute, fontSize: 12, paddingHorizontal: 8 },
		error: { color: color.danger },
		card: { paddingHorizontal: 10, paddingTop: 6, paddingBottom: 8 },
		input: { maxHeight: 140, minHeight: 40, color: color.fg, paddingHorizontal: 6, paddingTop: 10, paddingBottom: 6, fontSize: 16 },
		bar: { flexDirection: "row", alignItems: "center", gap: 4 },
		tool: { width: 40, height: 40, alignItems: "center", justifyContent: "center" },
		sendButton: { width: 40, height: 40, borderRadius: 20, backgroundColor: color.accent, alignItems: "center", justifyContent: "center" },
	}),
);
