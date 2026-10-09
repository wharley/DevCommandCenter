import {
	createThreadState,
	groupMessages,
	isThreadRunning,
	threadTitle,
	workspaceLabel,
	type RenderItem,
	type SessionSearchResult,
} from "@dcc/mobile-core";
import { useBackend } from "@/backend/session";
import { errorMessage, isAlreadyResolved } from "@/backend/types";
import { AssistantBlock, PermissionCard, QuestionCard, SystemRow, TraceBlock, UserBubble } from "@/screens/ThreadParts";
import { ThreadSync, type ThreadSnapshot } from "@/screens/threadSync";
import { alpha, color, radius } from "@/theme";
import { ConnectionBanner } from "@/ui/ConnectionBanner";
import { Loading, Mono, StateDot } from "@/ui/primitives";
import Feather from "@expo/vector-icons/Feather";
import { router, useLocalSearchParams } from "expo-router";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
	ActivityIndicator,
	AppState,
	FlatList,
	KeyboardAvoidingView,
	Platform,
	Pressable,
	StyleSheet,
	Text,
	TextInput,
	View,
	type NativeScrollEvent,
	type NativeSyntheticEvent,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

export default function ThreadScreen() {
	const { id } = useLocalSearchParams<{ id: string }>();
	const threadId = String(id);
	const backend = useBackend();
	const insets = useSafeAreaInsets();
	const [snapshot, setSnapshot] = useState<ThreadSnapshot>({
		state: createThreadState(),
		questions: [],
		loading: true,
		error: null,
		lastSynced: null,
	});
	const [meta, setMeta] = useState<SessionSearchResult | null>(null);
	const [composer, setComposer] = useState("");
	const [sending, setSending] = useState(false);
	const [stopping, setStopping] = useState(false);
	const [busy, setBusy] = useState<Set<string>>(() => new Set());
	const [notice, setNotice] = useState<string | null>(null);
	const sync = useRef<ThreadSync | null>(null);
	const list = useRef<FlatList<RenderItem>>(null);
	const stick = useRef(true);

	useEffect(() => {
		const controller = new ThreadSync(backend, threadId, setSnapshot);
		sync.current = controller;
		controller.start();
		const appState = AppState.addEventListener("change", (state) => {
			if (state === "active") controller.nudge();
		});
		return () => {
			appState.remove();
			controller.stop();
			sync.current = null;
		};
	}, [backend, threadId]);

	// No per-session metadata endpoint: find the row in the index once.
	useEffect(() => {
		let cancelled = false;
		backend
			.request<SessionSearchResult[]>("GET", "/api/v1/sessions/search?limit=120")
			.then((rows) => {
				if (!cancelled) setMeta(rows.find((row) => row.sessionId === threadId) ?? null);
			})
			.catch(() => {});
		return () => {
			cancelled = true;
		};
	}, [backend, threadId]);

	const { state, questions } = snapshot;
	const items = useMemo(() => groupMessages(state.messages), [state.messages]);
	const running = useMemo(() => isThreadRunning(state.messages), [state.messages]);
	const title = useMemo(() => meta?.threadTitle?.trim() || threadTitle(state.messages) || "Sessão", [meta, state.messages]);
	const place = meta ? workspaceLabel(meta) : "";
	const waitingOnYou = questions.length > 0 || state.messages.some((m) => m.kind === "permission" && !m.resolved);

	const withBusy = useCallback(async (key: string, run: () => Promise<void>) => {
		setBusy((prev) => new Set(prev).add(key));
		try {
			await run();
		} finally {
			setBusy((prev) => {
				const next = new Set(prev);
				next.delete(key);
				return next;
			});
		}
	}, []);

	const answerFailed = (error: unknown) => {
		if (isAlreadyResolved(error)) setNotice("Esse pedido já tinha sido respondido em outro lugar.");
		else setNotice(errorMessage(error, "Falha ao responder."));
		sync.current?.nudge();
	};

	const respondPermission = (requestId: string, choice: string) =>
		withBusy(requestId, async () => {
			try {
				await backend.request("POST", `/api/v1/sessions/${encodeURIComponent(threadId)}/respond-permission`, {
					sessionId: threadId,
					requestId,
					behavior: choice,
				});
				sync.current?.nudge();
			} catch (error) {
				answerFailed(error);
			}
		});

	const respondQuestion = (requestId: string, answers: Array<{ question: string; answer: string }>) =>
		withBusy(requestId, async () => {
			try {
				await backend.request("POST", `/api/v1/sessions/${encodeURIComponent(threadId)}/respond-user-input`, {
					sessionId: threadId,
					requestId,
					answers,
				});
				sync.current?.nudge();
			} catch (error) {
				answerFailed(error);
			}
		});

	const send = async () => {
		const prompt = composer.trim();
		if (!prompt || sending || running) return;
		setSending(true);
		setNotice(null);
		try {
			await backend.request("POST", `/api/v1/sessions/${encodeURIComponent(threadId)}/turns`, { sessionId: threadId, prompt });
			setComposer("");
			stick.current = true;
			sync.current?.nudge();
		} catch (error) {
			setNotice(errorMessage(error, "Falha ao enviar."));
		} finally {
			setSending(false);
		}
	};

	const stop = async () => {
		if (stopping) return;
		setStopping(true);
		try {
			await backend.request("POST", `/api/v1/sessions/${encodeURIComponent(threadId)}/abort`, { sessionId: threadId });
			sync.current?.nudge();
		} catch (error) {
			setNotice(errorMessage(error, "Falha ao parar."));
		} finally {
			setStopping(false);
		}
	};

	const followEnd = () => {
		if (stick.current) requestAnimationFrame(() => list.current?.scrollToEnd({ animated: false }));
	};

	const onScroll = (event: NativeSyntheticEvent<NativeScrollEvent>) => {
		const { contentOffset, contentSize, layoutMeasurement } = event.nativeEvent;
		stick.current = contentSize.height - contentOffset.y - layoutMeasurement.height < 120;
	};

	const renderItem = ({ item }: { item: RenderItem }) => {
		if (item.type === "trace") return <TraceBlock steps={item.steps} />;
		const message = item.message;
		switch (message.kind) {
			case "user":
				return <UserBubble text={message.text} />;
			case "assistant":
				return <AssistantBlock message={message} />;
			case "system":
				return <SystemRow text={message.text} />;
			case "permission":
				return (
					<PermissionCard
						message={message}
						busy={busy.has(message.requestId)}
						onRespond={(choice) => void respondPermission(message.requestId, choice)}
					/>
				);
			default:
				return <TraceBlock steps={[message]} />;
		}
	};

	return (
		<KeyboardAvoidingView style={styles.root} behavior={Platform.OS === "ios" ? "padding" : undefined}>
			<View style={[styles.header, { paddingTop: insets.top + 6 }]}>
				<Pressable onPress={() => router.back()} hitSlop={10} accessibilityRole="button" accessibilityLabel="Voltar" style={styles.back}>
					<Feather name="arrow-left" size={18} color={color.mute} />
				</Pressable>
				<View style={{ flex: 1, minWidth: 0 }}>
					<Text style={styles.title} numberOfLines={1}>
						{title}
					</Text>
					<View style={styles.subtitle}>
						<StateDot state={waitingOnYou ? "wait" : running ? "live" : "idle"} size={6} />
						<Mono style={styles.subtitleText} numberOfLines={1}>
							{[waitingOnYou ? "precisa de você" : running ? "rodando" : "parado", place].filter(Boolean).join(" · ")}
						</Mono>
					</View>
				</View>
			</View>
			<View style={styles.banner}>
				<ConnectionBanner backend={backend} />
				{snapshot.error ? <Text style={styles.error}>{snapshot.error}</Text> : null}
				{notice ? (
					<Pressable onPress={() => setNotice(null)}>
						<Text style={styles.notice}>{notice}</Text>
					</Pressable>
				) : null}
			</View>
			{snapshot.loading && items.length === 0 ? (
				<Loading />
			) : (
				<FlatList
					ref={list}
					data={items}
					keyExtractor={(item) => item.key}
					renderItem={renderItem}
					contentContainerStyle={styles.listContent}
					ItemSeparatorComponent={() => <View style={{ height: 12 }} />}
					onScroll={onScroll}
					scrollEventThrottle={64}
					onContentSizeChange={followEnd}
					// The keyboard closing resizes the list without changing its content.
					onLayout={followEnd}
					keyboardShouldPersistTaps="handled"
					ListFooterComponent={
						questions.length > 0 ? (
							<View style={{ gap: 12, marginTop: 12 }}>
								{questions.map((request) => (
									<QuestionCard
										key={request.requestId}
										request={request}
										busy={busy.has(request.requestId)}
										onSubmit={(answers) => void respondQuestion(request.requestId, answers)}
									/>
								))}
							</View>
						) : null
					}
				/>
			)}
			<View style={[styles.composer, { paddingBottom: Math.max(insets.bottom, 10) }]}>
				<TextInput
					value={composer}
					onChangeText={setComposer}
					placeholder={running ? "O agente está trabalhando…" : "Mensagem para o agente"}
					placeholderTextColor={color.faint}
					style={styles.input}
					multiline
					// Prompts carry identifiers and commands; autocorrect mangles them.
					autoCorrect={false}
					editable={!sending}
					accessibilityLabel="Mensagem para o agente"
				/>
				{running ? (
					<Pressable
						onPress={() => void stop()}
						style={[styles.sendButton, styles.stopButton]}
						accessibilityRole="button"
						accessibilityLabel="Parar o agente"
					>
						{stopping ? <ActivityIndicator size="small" color={color.danger} /> : <Feather name="square" size={16} color={color.danger} />}
					</Pressable>
				) : (
					<Pressable
						onPress={() => void send()}
						disabled={!composer.trim() || sending}
						style={[styles.sendButton, { opacity: !composer.trim() || sending ? 0.4 : 1 }]}
						accessibilityRole="button"
						accessibilityLabel="Enviar"
					>
						{sending ? <ActivityIndicator size="small" color={color.accentInk} /> : <Feather name="arrow-up" size={18} color={color.accentInk} />}
					</Pressable>
				)}
			</View>
		</KeyboardAvoidingView>
	);
}

const styles = StyleSheet.create({
	root: { flex: 1, backgroundColor: color.bg },
	header: {
		flexDirection: "row",
		alignItems: "center",
		gap: 10,
		paddingHorizontal: 12,
		paddingBottom: 10,
		borderBottomWidth: 1,
		borderBottomColor: color.border,
	},
	back: { padding: 6 },
	title: { color: color.fg, fontSize: 15, fontWeight: "600" },
	subtitle: { flexDirection: "row", alignItems: "center", gap: 6, marginTop: 3 },
	subtitleText: { fontSize: 10, color: color.faint, letterSpacing: 0.6, flexShrink: 1 },
	banner: { paddingHorizontal: 12, paddingTop: 10 },
	error: { color: color.danger, fontSize: 12, marginBottom: 8 },
	notice: { color: color.mute, fontSize: 12, marginBottom: 8 },
	listContent: { paddingHorizontal: 16, paddingBottom: 20, paddingTop: 4 },
	composer: {
		flexDirection: "row",
		alignItems: "flex-end",
		gap: 8,
		paddingHorizontal: 12,
		paddingTop: 10,
		borderTopWidth: 1,
		borderTopColor: color.border,
		backgroundColor: color.panel,
	},
	input: {
		flex: 1,
		maxHeight: 140,
		minHeight: 40,
		borderWidth: 1,
		borderColor: color.border,
		backgroundColor: color.bg,
		borderRadius: radius.md,
		color: color.fg,
		paddingHorizontal: 12,
		paddingTop: 10,
		paddingBottom: 10,
		fontSize: 15,
	},
	sendButton: {
		width: 40,
		height: 40,
		borderRadius: 20,
		backgroundColor: color.accent,
		alignItems: "center",
		justifyContent: "center",
	},
	stopButton: { backgroundColor: alpha(color.danger, 0.12), borderWidth: 1, borderColor: alpha(color.danger, 0.5) },
});
