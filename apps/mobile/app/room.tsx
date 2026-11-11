/**
 * A room on the phone, as on the Mac: Geral, or a project's (`?room=<id>`,
 * `&name=`), one conversation with the resident agents. The person writes
 * once; with `@Nome` they pick who answers, without it the desktop's rule
 * does. Answers read as flat rows under each agent's mascot; an approval an
 * agent waits on is answered right on the field; files, photos and
 * dictation go in as in a task. `?to=Nome` opens with "@Nome " written.
 *
 * The list is inverted (newest first): the room opens at its latest lines
 * with no jump, follows what comes while the person is at the end, and
 * stays put when they scrolled up, where "N novas" takes them back. Each
 * agent at work shows live at the end (`roomLive`), with the turns that
 * stopped and the messages queued. Rows are memoized and a read keeps the
 * objects that did not change, so typing or a read redraws only what moved.
 */
import {
	applyFileMention,
	findActiveFileMention,
	linesSince,
	matchingAgents,
	queuedLabel,
	roomItems,
	roomReasonLabel,
	t,
	type ResidentAgent,
	type RoomApproval,
	type RoomCard,
	type RoomLine,
	type RoomLive,
	type RoomQueued,
	type RoomQuestion,
	type RoomReaction,
	type RoomStopped,
	type RoomTask,
} from "@dcc/mobile-core";
import { useResidentAgents } from "@/backend/residentAgents";
import { markRoomSeen } from "@/backend/roomsSeen";
import { useBackend } from "@/backend/session";
import { sendError, useTeamRoom, type ApprovalChoice } from "@/backend/teamRoom";
import { useDictation } from "@/dictation/useDictation";
import { AgentLine, ApprovalDock, DayPill, MentionList, PersonLine, WorkingLine } from "@/screens/roomParts";
import { JumpPill, LiveRow, StoppedRow } from "@/screens/roomLive";
import { RoomCardRow } from "@/screens/roomCards";
import { RuleSheet } from "@/screens/roomReactions";
import { QuestionDock } from "@/screens/roomQuestion";
import { makeStyles, useTheme } from "@/theme";
import { AttachButton, AttachmentTray, DictationStatus, MicButton, useAttachments } from "@/ui/ComposerTools";
import { ConnectionBanner } from "@/ui/ConnectionBanner";
import { Glass, GlassButton } from "@/ui/glass";
import { haptic } from "@/ui/haptics";
import { Loading, Mono } from "@/ui/primitives";
import { EdgeFade } from "@/ui/surfaces";
import Feather from "@expo/vector-icons/Feather";
import { router, useIsFocused, useLocalSearchParams } from "expo-router";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
	ActivityIndicator,
	FlatList,
	Linking,
	Pressable,
	StyleSheet,
	Text,
	TextInput,
	View,
	type NativeScrollEvent,
	type NativeSyntheticEvent,
} from "react-native";
import { KeyboardAvoidingView, useKeyboardState } from "react-native-keyboard-controller";
import { useSafeAreaInsets } from "react-native-safe-area-context";

/** One row of the room's list, with the air above it. */
type RowBody = { key: string } & (
	| { type: "day"; label: string }
	| { type: "line"; line: RoomLine; first: boolean }
	| { type: "card"; card: RoomCard }
	| { type: "stopped"; stopped: RoomStopped }
	| { type: "live"; live: RoomLive }
	| { type: "working"; agent: ResidentAgent }
	| { type: "queued"; queued: RoomQueued }
);
type Row = RowBody & { gap: number };

/** Scrolled this far from the end, the room stops following. */
const FOLLOW_SLACK = 64;

/**
 * The room's lines in time order with its cards among them: a pill at each
 * new day, an answer within its group close, a new speaker or a card with
 * air; DCC's own cards (a limit, a proposal) after them all.
 */
function lineRows(lines: RoomLine[], cards: RoomCard[]): Row[] {
	return roomItems(lines, new Date(), cards).map((item, index): Row => {
		const gap = index === 0 ? 0 : item.type === "line" && !item.first ? 8 : item.type === "card" ? 14 : 18;
		switch (item.type) {
			case "day":
				return { type: "day", key: item.key, label: item.label, gap };
			case "card":
				return { type: "card", key: item.key, card: item.card, gap };
			case "line":
				return { type: "line", key: item.key, line: item.line, first: item.first, gap };
		}
	});
}

/**
 * After the lines: the stops to try again, the agents at work and the
 * messages queued; then all of it newest first, as the inverted list reads
 * it. The lines' rows are built apart, so a beat of the turns at work keeps them.
 */
function roomRows(lines: Row[], stopped: RoomStopped[], live: RoomLive[], working: ResidentAgent[], queued: RoomQueued[]): Row[] {
	const after: Row[] = [];
	const push = (row: RowBody, gap: number) => after.push({ ...row, gap: lines.length + after.length === 0 ? 0 : gap });
	for (const stop of stopped) push({ type: "stopped", key: `stopped-${stop.sessionId}-${stop.turnId}`, stopped: stop }, 14);
	for (const turn of live) push({ type: "live", key: `live-${turn.key}`, live: turn }, 18);
	// An older desktop says only who works: the plain line.
	for (const agent of working) push({ type: "working", key: `working-${agent.id}`, agent }, 18);
	for (const message of queued) push({ type: "queued", key: `queued-${message.id}`, queued: message }, 14);
	return [...lines, ...after].reverse();
}

export default function TeamRoomScreen() {
	const backend = useBackend();
	const { color } = useTheme();
	const styles = useStyles();
	const insets = useSafeAreaInsets();
	const params = useLocalSearchParams<{ to?: string; room?: string; name?: string }>();
	const roomId = params.room || null;
	const roomName = roomId ? params.name || "Projeto" : "Geral";
	const agents = useResidentAgents();
	const focused = useIsFocused();
	const room = useTeamRoom(backend, roomId, focused);
	const [text, setText] = useState(params.to ? `@${params.to} ` : "");
	const [selection, setSelection] = useState({ start: text.length, end: text.length });
	const [forced, setForced] = useState<{ start: number; end: number } | undefined>(undefined);
	const [sending, setSending] = useState(false);
	const [notice, setNotice] = useState<{ text: string; error: boolean } | null>(null);
	const [dockHeight, setDockHeight] = useState(120);
	const attachments = useAttachments(backend);
	const dictation = useDictation(text, setText);
	const input = useRef<TextInput>(null);
	const list = useRef<FlatList<Row>>(null);
	const [scrolledUp, setScrolledUp] = useState(false);
	const up = useRef(false);
	const [seen, setSeen] = useState<string | null>(null);
	const keyboardOpen = useKeyboardState((keyboard) => keyboard.isVisible);

	const byId = useMemo(() => new Map(agents.map((agent) => [agent.id, agent])), [agents]);
	const mention = findActiveFileMention(text, selection.end);
	const matches = mention ? matchingAgents(agents, mention.query) : [];
	// Who works in this room; an older desktop only says how each agent is anywhere.
	const working = useMemo(
		() =>
			room.working
				? agents.filter((agent) => room.working!.includes(agent.id))
				: roomId
					? []
					: agents.filter((agent) => agent.state === "working"),
		[agents, room.working, roomId],
	);
	const lined = useMemo(() => lineRows(room.lines, room.cards), [room.lines, room.cards]);
	const rows = useMemo(() => {
		const atWork = new Set(room.live.map((turn) => turn.agentId));
		return roomRows(
			lined,
			room.stopped.filter((stop) => byId.has(stop.agentId)),
			room.live.filter((turn) => byId.has(turn.agentId)),
			working.filter((agent) => !atWork.has(agent.id)),
			room.queued,
		);
	}, [lined, room.stopped, room.live, room.queued, working, byId]);

	const lastAt = room.lines.at(-1)?.at ?? null;
	useEffect(() => {
		// Open is read: what is on screen stops being news on the home.
		if (!room.loading) markRoomSeen(roomId, lastAt);
	}, [roomId, lastAt, room.loading]);

	// At the end the newest line is seen; scrolled up, what comes is counted.
	const newest = room.lines.at(-1)?.key ?? null;
	useEffect(() => {
		if (!scrolledUp) setSeen(newest);
	}, [newest, scrolledUp]);
	const unseen = scrolledUp ? linesSince(room.lines, seen) : 0;

	const onScroll = useCallback((event: NativeSyntheticEvent<NativeScrollEvent>) => {
		// Inverted: the end of the room is offset 0.
		const away = event.nativeEvent.contentOffset.y > FOLLOW_SLACK;
		if (away !== up.current) {
			up.current = away;
			setScrolledUp(away);
		}
	}, []);
	const toEnd = useCallback(() => list.current?.scrollToOffset({ offset: 0, animated: true }), []);

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
			toEnd();
			setNotice({
				text: sent.queued ? queuedLabel(sent.agentName) : `Foi para ${sent.agentName} · ${roomReasonLabel(sent.reason)}`,
				error: false,
			});
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

	const answer = (approval: RoomApproval, choice: ApprovalChoice, reason?: string) => {
		haptic(choice === "deny" ? "warning" : "success");
		void room.answer(approval, choice, reason).then((failed) => failed && setNotice({ text: failed, error: true }));
	};

	// Stable, so a row redraws only when its own line moves.
	const { retry, stop, handoff, steps: loadSteps, react, answerProposal, continueLimit, keepRule } = room;
	/** The answer whose 👎 asks "Guardar como regra?". */
	const [rule, setRule] = useState<{ agentId: string } | null>(null);
	const failed = useCallback((text: string | null) => {
		if (text) setNotice({ text, error: true });
	}, []);
	const openLine = useCallback((line: RoomLine) => router.push({ pathname: "/thread/[id]", params: { id: line.sessionId } }), []);
	const openTask = useCallback(
		(task: RoomTask, where: "thread" | "delivery") =>
			where === "thread" && task.sessionId
				? router.push({ pathname: "/thread/[id]", params: { id: task.sessionId } })
				: router.push({ pathname: "/delivery/[id]", params: { id: task.workspaceId } }),
		[],
	);
	const openLive = useCallback(
		(turn: RoomLive) =>
			turn.kind === "task" && turn.task?.workspaceId && !turn.sessionId
				? router.push({ pathname: "/delivery/[id]", params: { id: turn.task.workspaceId } })
				: router.push({ pathname: "/thread/[id]", params: { id: turn.sessionId } }),
		[],
	);
	const onHandoff = useCallback(
		(line: RoomLine, action: "pass" | "ignore") => {
			void handoff(line, action).then(failed);
			if (action === "pass") toEnd();
		},
		[handoff, failed, toEnd],
	);
	const onStop = useCallback((turn: RoomLive) => void stop(turn).then(failed), [stop, failed]);
	const onReact = useCallback(
		(line: RoomLine, reaction: RoomReaction | null) => {
			void react(line, reaction).then((error) => {
				failed(error);
				if (!error && reaction === "down") setRule({ agentId: line.agentId });
			});
		},
		[react, failed],
	);
	const onKeepRule = (body: string) => {
		if (!rule) return;
		void keepRule(rule.agentId, body).then((error) => {
			if (error) {
				setNotice({ text: error, error: true });
				return;
			}
			setRule(null);
			setNotice({ text: t("agents.reactions.kept"), error: false });
		});
	};
	const openCard = useCallback((card: RoomCard, where: "thread" | "delivery") => {
		if (where === "delivery" && card.workspaceId) {
			router.push({ pathname: "/delivery/[id]", params: { id: card.workspaceId } });
		} else if (card.sessionId) {
			router.push({ pathname: "/thread/[id]", params: { id: card.sessionId } });
		} else if (card.workspaceId) {
			router.push({ pathname: "/delivery/[id]", params: { id: card.workspaceId } });
		} else if (card.checkIn?.url) {
			void Linking.openURL(card.checkIn.url);
		}
	}, []);
	const actOnCard = useCallback(
		(card: RoomCard, action: "create" | "discard" | "continue") => {
			void (action === "continue" ? continueLimit(card) : answerProposal(card, action)).then(failed);
		},
		[answerProposal, continueLimit, failed],
	);
	const onRetry = useCallback(
		(turn: RoomStopped) => {
			void retry(turn).then(failed);
			toEnd();
		},
		[retry, failed, toEnd],
	);

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

	const answering = room.answering;
	const renderItem = useCallback(
		({ item }: { item: Row }) => {
			const row = (() => {
				switch (item.type) {
					case "day":
						return <DayPill label={item.label} />;
					case "line": {
						const agent = byId.get(item.line.agentId);
						if (item.line.speaker === "person") return <PersonLine line={item.line} agent={agent} team={agents} />;
						if (!agent) return null;
						return (
							<AgentLine
								line={item.line}
								agent={agent}
								first={item.first}
								handing={answering.has(`handoff:${item.line.key}`)}
								onOpen={openLine}
								onTask={openTask}
								onHandoff={onHandoff}
								loadSteps={loadSteps}
								onReact={onReact}
							/>
						);
					}
					case "card": {
						const agent = item.card.agentId ? byId.get(item.card.agentId) : undefined;
						// A card of an agent the phone does not know yet waits for the team's next read.
						if (item.card.agentId && !agent) return null;
						return <RoomCardRow card={item.card} agent={agent} busy={answering.has(`card:${item.card.key}`)} onOpen={openCard} onAct={actOnCard} />;
					}
					case "stopped": {
						const agent = byId.get(item.stopped.agentId);
						return agent ? (
							<StoppedRow stopped={item.stopped} agent={agent} busy={answering.has(`retry:${item.stopped.turnId}`)} onRetry={onRetry} />
						) : null;
					}
					case "live": {
						const agent = byId.get(item.live.agentId);
						return agent ? (
							<LiveRow live={item.live} agent={agent} stopping={answering.has(`stop:${item.live.sessionId}`)} onStop={onStop} onOpen={openLive} />
						) : null;
					}
					case "working":
						return <WorkingLine agent={item.agent} />;
					case "queued": {
						const agent = byId.get(item.queued.agentId);
						return (
							<PersonLine line={{ text: item.queued.text, wake: null }} agent={agent} team={agents} note={queuedLabel(agent?.name ?? "agente")} />
						);
					}
				}
			})();
			return row ? <View style={item.gap ? { paddingTop: item.gap } : undefined}>{row}</View> : null;
		},
		[byId, agents, answering, openLine, openTask, onHandoff, loadSteps, onReact, openCard, actOnCard, onRetry, onStop, openLive],
	);

	return (
		<KeyboardAvoidingView style={styles.root} behavior="padding">
			<View style={styles.frame}>
				{room.loading && rows.length === 0 ? (
					<View style={{ flex: 1, paddingTop: headerSpace }}>
						<Loading />
					</View>
				) : rows.length === 0 ? (
					<View style={[styles.listContent, { flex: 1, paddingTop: headerSpace }]}>
						<View style={{ marginBottom: 8 }}>
							<ConnectionBanner backend={backend} />
						</View>
						<Text style={styles.empty}>
							{roomId
								? `Ninguém falou em ${roomName} ainda. Peça algo do projeto: aqui os agentes já sabem que é ele.`
								: "Ninguém falou ainda. Pergunte o que precisa de você hoje ou peça para o @Revisor olhar um PR."}
						</Text>
					</View>
				) : (
					<FlatList
						ref={list}
						inverted
						data={rows}
						keyExtractor={(item) => item.key}
						renderItem={renderItem}
						// Inverted: the top padding sits at the end, over the field.
						contentContainerStyle={[styles.listContent, { paddingTop: dockHeight + dockBottom + 20, paddingBottom: headerSpace }]}
						keyboardShouldPersistTaps="handled"
						onScroll={onScroll}
						scrollEventThrottle={64}
						// Scrolled up, what comes in does not move what is being read.
						maintainVisibleContentPosition={scrolledUp ? { minIndexForVisible: 0 } : undefined}
						initialNumToRender={14}
						windowSize={9}
						ListFooterComponent={
							<View style={{ marginBottom: 8 }}>
								<ConnectionBanner backend={backend} />
							</View>
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

				<RuleSheet
					visible={rule !== null}
					busy={rule !== null && answering.has(`rule:${rule.agentId}`)}
					onKeep={onKeepRule}
					onClose={() => setRule(null)}
				/>

				<JumpPill count={unseen} bottom={dockBottom + dockHeight + 12} onPress={toEnd} />

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
