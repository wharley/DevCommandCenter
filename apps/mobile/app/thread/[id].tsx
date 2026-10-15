import {
	accessPolicies,
	agentTurnHeads,
	applyFileMention,
	createThreadState,
	findActiveFileMention,
	groupMessages,
	isAccessPolicy,
	isThreadRunning,
	normalizeProvider,
	pickAccessPolicy,
	proposalsForSession,
	threadApprovalPolicy,
	t,
	stoppedByUsageLimit,
	threadTitle,
	workspaceDiffStat,
	type AccessPolicy,
	type DeliverySummary,
	type MobileCatalog,
	type RenderItem,
	type SessionSearchResult,
} from "@dcc/mobile-core";
import { useAgentRequests } from "@/backend/agentRequests";
import { useTaskProposals } from "@/backend/taskProposals";
import { loadProjectAccess, saveProjectAccess } from "@/backend/accessStore";
import { markWorkspaceSeen } from "@/backend/seenResults";
import { cachedCatalog, fetchCatalog } from "@/backend/catalogCache";
import { useBackend } from "@/backend/session";
import { errorMessage, isAlreadyResolved } from "@/backend/types";
import { AssistantBlock, PermissionCard, QuestionCard, SystemRow, TraceBlock, UserBubble } from "@/screens/ThreadParts";
import { ThreadSync, type ThreadSnapshot } from "@/screens/threadSync";
import { alpha, makeStyles, useTheme } from "@/theme";
import { AccessPill } from "@/ui/AccessPicker";
import { AgentRequestCard } from "@/ui/AgentRequestCard";
import { TaskProposalCard } from "@/ui/TaskProposalCard";
import { ConnectionBanner } from "@/ui/ConnectionBanner";
import { AttachButton, AttachmentTray, DictationStatus, MicButton, useAttachments } from "@/ui/ComposerTools";
import { FileMentionList } from "@/ui/FileMentionList";
import { useDictation } from "@/dictation/useDictation";
import { Glass, GlassButton, GlassPill } from "@/ui/glass";
import { haptic } from "@/ui/haptics";
import { Button, Loading, Mono, StateDot } from "@/ui/primitives";
import { EdgeFade } from "@/ui/surfaces";
import { AgentMascot } from "@/ui/sprites";
import Feather from "@expo/vector-icons/Feather";
import { router, useLocalSearchParams } from "expo-router";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
	ActivityIndicator,
	AppState,
	FlatList,
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

export default function ThreadScreen() {
	const { color } = useTheme();
	const styles = useStyles();
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
	const [resuming, setResuming] = useState(false);
	const [busy, setBusy] = useState<Set<string>>(() => new Set());
	const [notice, setNotice] = useState<string | null>(null);
	const [catalog, setCatalog] = useState<MobileCatalog | null>(() => cachedCatalog(backend));
	/** Picked here; until then the session keeps the level of its latest turn. */
	const [chosenAccess, setChosenAccess] = useState<AccessPolicy | null>(null);
	const [rememberedAccess, setRememberedAccess] = useState<AccessPolicy | null>(null);
	const [selection, setSelection] = useState({ start: 0, end: 0 });
	const [forcedSelection, setForcedSelection] = useState<{ start: number; end: number } | undefined>();
	const [composerFocused, setComposerFocused] = useState(false);
	/** Height of the floating composer, so the list ends above it. */
	const [dockHeight, setDockHeight] = useState(120);
	const [diff, setDiff] = useState<ReturnType<typeof workspaceDiffStat> | null>(null);
	const attachments = useAttachments(backend);
	const dictation = useDictation(composer, setComposer);
	const input = useRef<TextInput>(null);
	const sync = useRef<ThreadSync | null>(null);
	const list = useRef<FlatList<RenderItem>>(null);
	const stick = useRef(true);
	const listSize = useRef({ content: 0, visible: 0 });

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

	// What the agent enforces (`capabilities.approvalPolicies`) comes with the catalog.
	useEffect(() => {
		let cancelled = false;
		fetchCatalog(backend)
			.then((next) => {
				if (!cancelled) setCatalog(next);
			})
			.catch(() => {});
		return () => {
			cancelled = true;
		};
	}, [backend]);

	// Opening the thread reads the task's latest result ("Resultado novo" goes away).
	const seenWorkspace = meta?.workspaceId ?? null;
	const finished = !isThreadRunning(snapshot.state.messages) && !snapshot.loading;
	useEffect(() => {
		if (seenWorkspace && finished) markWorkspaceSeen(seenWorkspace);
	}, [seenWorkspace, finished]);

	// The diff chip: what the workspace changed, read on open and again when a turn starts or ends.
	const diffWorkspace = meta?.workspaceId ?? null;
	const turnRunning = isThreadRunning(snapshot.state.messages);
	useEffect(() => {
		if (!diffWorkspace) return;
		let cancelled = false;
		backend
			.request<DeliverySummary>("GET", `/api/v1/mobile/workspaces/${encodeURIComponent(diffWorkspace)}/delivery`)
			.then((summary) => {
				if (!cancelled) setDiff(workspaceDiffStat(summary));
			})
			.catch(() => {});
		return () => {
			cancelled = true;
		};
	}, [backend, diffWorkspace, turnRunning]);

	const projectKey = meta?.projectId ?? null;
	useEffect(() => {
		let cancelled = false;
		if (!projectKey) return;
		void loadProjectAccess(projectKey).then((policy) => {
			if (!cancelled) setRememberedAccess(policy);
		});
		return () => {
			cancelled = true;
		};
	}, [projectKey]);

	const agentRequests = useAgentRequests();
	const threadRequests = useMemo(
		() => agentRequests.requests.filter((request) => request.sessionId === threadId),
		[agentRequests.requests, threadId],
	);
	const taskProposals = useTaskProposals();
	const threadProposals = useMemo(
		() => proposalsForSession(taskProposals.proposals, threadId),
		[taskProposals.proposals, threadId],
	);
	const { state, questions } = snapshot;
	const items = useMemo(() => groupMessages(state.messages), [state.messages]);
	const running = useMemo(() => isThreadRunning(state.messages), [state.messages]);
	const title = useMemo(() => meta?.threadTitle?.trim() || threadTitle(state.messages) || "Sessão", [meta, state.messages]);
	const waitingOnYou =
		questions.length > 0 ||
		threadRequests.length > 0 ||
		threadProposals.length > 0 ||
		state.messages.some((m) => m.kind === "permission" && !m.resolved);
	const keyboardOpen = useKeyboardState((keyboard) => keyboard.isVisible);
	const heads = useMemo(() => agentTurnHeads(items), [items]);
	// Only the latest turn moves; finished replies stand still, as on the desktop.
	const lastItem = items.at(-1);
	const awaitingFirstOutput =
		running && lastItem?.type === "single" && lastItem.message.kind === "user";
	const liveHead = (running || waitingOnYou) && !awaitingFirstOutput ? heads.at(-1) : undefined;
	const mascotState = waitingOnYou ? "waiting" : "working";
	const agentLabel = meta?.model || (meta?.providerId ? normalizeProvider(meta.providerId) : null);
	const provider = catalog?.providers.find((p) => p.id === meta?.providerId) ?? null;
	const availableAccess = useMemo(() => accessPolicies(provider), [provider]);
	const recorded = threadApprovalPolicy(state.messages);
	const sessionAccess = isAccessPolicy(recorded) && availableAccess.includes(recorded) ? recorded : null;
	const access = provider ? pickAccessPolicy(availableAccess, chosenAccess, sessionAccess, rememberedAccess) : null;
	const chooseAccess = (policy: AccessPolicy) => {
		setChosenAccess(policy);
		if (projectKey) saveProjectAccess(projectKey, policy);
	};

	const mention = composerFocused && meta?.workspaceId ? findActiveFileMention(composer, selection.end) : null;
	const insertMention = (path: string) => {
		if (!mention) return;
		const next = applyFileMention(composer, mention, path);
		setComposer(next.text);
		setForcedSelection({ start: next.cursor, end: next.cursor });
		setSelection({ start: next.cursor, end: next.cursor });
		// Tapping the list takes focus away; keep typing where the mention ended.
		input.current?.focus();
	};

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

	const uploading = attachments.items.some((item) => item.status === "uploading");
	const hasReadyAttachment = attachments.items.some((item) => item.status === "ready");
	const canSend = (composer.trim().length > 0 || hasReadyAttachment) && !uploading && !sending;
	const imagesUnsupported =
		provider?.capabilities?.vision === false && attachments.items.some((item) => item.kind === "image");

	const send = async () => {
		if (!canSend || running) return;
		// Dictation never sends by itself; sending stops it with what was heard.
		if (dictation.listening) dictation.stop();
		const prompt = attachments.queue.prompt(composer);
		if (!prompt) return;
		setSending(true);
		setNotice(null);
		try {
			// Without a pick here the desktop keeps the session's latest level;
			// a session with none recorded gets the level shown in the pill.
			const approvalPolicy = chosenAccess ?? (sessionAccess ? undefined : (access ?? undefined));
			await backend.request("POST", `/api/v1/sessions/${encodeURIComponent(threadId)}/turns`, {
				sessionId: threadId,
				prompt,
				...(approvalPolicy ? { approvalPolicy } : {}),
			});
			setComposer("");
			attachments.queue.clear();
			stick.current = true;
			sync.current?.nudge();
		} catch (error) {
			setNotice(errorMessage(error, "Falha ao enviar."));
		} finally {
			setSending(false);
		}
	};

	/** "Continuar" after the plan's usage limit: the desktop sends its fixed continuation. */
	const resumeAfterLimit = async () => {
		if (resuming) return;
		setResuming(true);
		try {
			await backend.request("POST", `/api/v1/sessions/${encodeURIComponent(threadId)}/usage-limit/resume`, {});
			stick.current = true;
			sync.current?.nudge();
		} catch (error) {
			setNotice(errorMessage(error, "Falha ao retomar."));
		} finally {
			setResuming(false);
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

	/**
	 * To the last message, from the sizes the list reported. `scrollToEnd` keeps
	 * the height from before the keyboard shrank the list and stops short.
	 */
	const scrollToBottom = (animated: boolean) => {
		const { content, visible } = listSize.current;
		list.current?.scrollToOffset({ offset: Math.max(0, content - visible), animated });
	};

	const followEnd = () => {
		if (stick.current) requestAnimationFrame(() => scrollToBottom(false));
	};


	// The keyboard shrinks the list once its animation ends; then keep the latest
	// message in view above the composer.
	useEffect(() => {
		if (!keyboardOpen) return;
		stick.current = true;
		const timer = setTimeout(() => scrollToBottom(true), 450);
		return () => clearTimeout(timer);
	}, [keyboardOpen]);

	const onScroll = (event: NativeSyntheticEvent<NativeScrollEvent>) => {
		const { contentOffset, contentSize, layoutMeasurement } = event.nativeEvent;
		stick.current = contentSize.height - contentOffset.y - layoutMeasurement.height < 120;
	};

	const renderItem = ({ item }: { item: RenderItem }) => {
		if (!heads.includes(item.key)) return renderBody(item);
		return (
			<View style={{ gap: 8 }}>
				<AgentHeader meta={meta} label={agentLabel} state={item.key === liveHead ? mascotState : "idle"} />
				{renderBody(item)}
			</View>
		);
	};

	const renderBody = (item: RenderItem) => {
		if (item.type === "trace") return <TraceBlock steps={item.steps} />;
		const message = item.message;
		switch (message.kind) {
			case "user":
				return <UserBubble text={message.text} backend={backend} sessionId={threadId} />;
			case "assistant":
				return <AssistantBlock message={message} />;
			case "system":
				return <SystemRow text={message.text} />;
			case "permission":
				return (
					<PermissionCard
						message={message}
						busy={busy.has(message.requestId)}
						onRespond={(choice) => {
							haptic(choice === "deny" ? "warning" : "success");
							void respondPermission(message.requestId, choice);
						}}
					/>
				);
			default:
				return <TraceBlock steps={[message]} />;
		}
	};

	const goBack = () => (router.canGoBack() ? router.back() : router.replace("/"));
	const openDelivery = meta?.workspaceId
		? () => router.push({ pathname: "/delivery/[id]", params: { id: meta.workspaceId! } })
		: undefined;
	const stateText = waitingOnYou ? "precisa de você" : running ? "rodando" : "parado";
	const dockBottom = keyboardOpen ? 8 : Math.max(insets.bottom, 8);
	const headerSpace = insets.top + 6 + 48 + (diff ? 44 : 0) + 6;
	const banner = (
		<View style={{ gap: 8, marginBottom: 8 }}>
			<ConnectionBanner backend={backend} />
			{snapshot.error ? <Text style={styles.error}>{snapshot.error}</Text> : null}
			{notice ? (
				<Pressable onPress={() => setNotice(null)}>
					<Text style={styles.notice}>{notice}</Text>
				</Pressable>
			) : null}
		</View>
	);

	return (
		<KeyboardAvoidingView style={styles.root} behavior="padding">
			{/* Padding shrinks this wrapper, not the floating chrome: absolute children
			    are placed against the inner view, so they rise with the keyboard. */}
			<View style={styles.frame}>
				{snapshot.loading && items.length === 0 ? (
					<View style={{ flex: 1, paddingTop: headerSpace }}>
						<Loading />
					</View>
				) : (
					<FlatList
						ref={list}
						data={items}
						keyExtractor={(item) => item.key}
						renderItem={renderItem}
						extraData={`${liveHead}/${mascotState}/${agentLabel}`}
						contentContainerStyle={[styles.listContent, { paddingTop: headerSpace, paddingBottom: dockHeight + dockBottom + 20 }]}
						scrollIndicatorInsets={{ top: headerSpace - insets.top, bottom: dockHeight }}
						ListHeaderComponent={banner}
						ItemSeparatorComponent={() => <View style={{ height: 12 }} />}
						onScroll={onScroll}
						scrollEventThrottle={64}
						onContentSizeChange={(_, height) => {
							listSize.current.content = height;
							followEnd();
						}}
						// The keyboard closing resizes the list without changing its content.
						onLayout={(event) => {
							listSize.current.visible = event.nativeEvent.layout.height;
							followEnd();
						}}
						keyboardShouldPersistTaps="handled"
						ListFooterComponent={
							awaitingFirstOutput ||
							questions.length > 0 ||
							threadRequests.length > 0 ||
							threadProposals.length > 0 ? (
								<View style={{ gap: 12, marginTop: 12 }}>
									{awaitingFirstOutput ? <AgentHeader meta={meta} label={agentLabel} state={mascotState} /> : null}
									{threadRequests.map((request) => (
										<AgentRequestCard
											key={request.requestId}
											request={request}
											busy={agentRequests.answering.has(request.requestId)}
											onRespond={(answer) =>
												void agentRequests.respond(request, answer).then((text) => {
													if (text) setNotice(text);
													sync.current?.nudge();
												})
											}
										/>
									))}
									{threadProposals.map((proposal) => (
										<TaskProposalCard
											key={proposal.id}
											proposal={proposal}
											busy={taskProposals.answering.has(proposal.id)}
											onAnswer={(action) =>
												void taskProposals.answer(proposal, action).then((text) => {
													if (text) setNotice(text);
													sync.current?.nudge();
												})
											}
										/>
									))}
									{questions.map((request) => (
										<QuestionCard
											key={request.requestId}
											request={request}
											busy={busy.has(request.requestId)}
											onSubmit={(answers) => {
												haptic("success");
												void respondQuestion(request.requestId, answers);
											}}
										/>
									))}
								</View>
							) : null
						}
					/>
				)}

				<EdgeFade edge="top" height={headerSpace + 16} solid={(headerSpace - 10) / (headerSpace + 16)} />
				<EdgeFade edge="bottom" height={dockHeight + dockBottom + 28} solid={0.45} />

				<View style={[styles.header, { top: insets.top + 6 }]}>
					{/* Opened from a notification on a cold start, the thread is the only screen. */}
					<GlassButton icon="chevron-left" accessibilityLabel="Voltar" onPress={goBack} />
					<View style={styles.titleBlock}>
						<Text style={styles.title} numberOfLines={1} accessibilityRole="header">
							{title}
						</Text>
						<View style={styles.subtitle}>
							<StateDot state={waitingOnYou ? "wait" : running ? "live" : "idle"} size={6} />
							<Mono style={styles.subtitleText} numberOfLines={1}>
								{[stateText, agentLabel].filter(Boolean).join(" · ")}
							</Mono>
						</View>
					</View>
					{openDelivery ? (
						<GlassButton icon="git-pull-request" accessibilityLabel="Entrega: commit, envio e PR" onPress={openDelivery} />
					) : (
						<View style={{ width: 44 }} />
					)}
				</View>

				{diff ? (
					<View style={[styles.diffRow, { top: insets.top + 60 }]} pointerEvents="box-none">
						<GlassPill
							onPress={openDelivery}
							accessibilityLabel={`${diff.branch ?? "Workspace"}: ${diff.files} arquivos alterados, ${diff.additions} linhas a mais, ${diff.deletions} a menos. Abrir entrega`}
							style={styles.diffPill}
						>
							<Feather name="git-branch" size={12} color={color.mute} />
							{diff.branch ? (
								<Mono style={styles.diffBranch} numberOfLines={1}>
									{diff.branch}
								</Mono>
							) : null}
							<Mono style={styles.diffText}>
								{diff.files === 0 ? "· sem mudanças" : `· ${diff.files} ${diff.files === 1 ? "arquivo" : "arquivos"}`}
							</Mono>
							{diff.files > 0 ? <Mono style={[styles.diffText, { color: color.accent }]}>+{diff.additions}</Mono> : null}
							{diff.files > 0 ? <Mono style={[styles.diffText, { color: color.danger }]}>−{diff.deletions}</Mono> : null}
							<Feather name="chevron-right" size={13} color={color.mute} />
						</GlassPill>
					</View>
				) : null}

				{/* With the keyboard up it already covers the home indicator / nav bar. */}
				<View style={[styles.dock, { bottom: dockBottom }]} onLayout={(event) => setDockHeight(event.nativeEvent.layout.height)}>
						{mention && meta?.workspaceId ? (
							<FileMentionList
								backend={backend}
								scope={{ workspaceId: meta.workspaceId }}
								query={mention.query}
								onPick={insertMention}
							/>
						) : null}
						{!running && stoppedByUsageLimit(state.messages) ? (
							<Button
								label={t("usageLimit.continue")}
								variant="ghost"
								busy={resuming}
								onPress={() => void resumeAfterLimit()}
								accessibilityLabel={t("usageLimit.continue")}
							/>
						) : null}
					<Glass radius={26} style={styles.capsule}>
						{provider ? (
							<AccessPill
								available={availableAccess}
								value={access}
								onChange={chooseAccess}
								providerLabel={provider.label}
								disabled={sending}
							/>
						) : null}
						<AttachmentTray attachments={attachments} />
						{imagesUnsupported ? <Text style={styles.notice}>{t("composer.attachments.imagesUnsupported")}</Text> : null}
						<DictationStatus dictation={dictation} />
						<View style={styles.composerRow}>
							<AttachButton attachments={attachments} disabled={sending} />
							<TextInput
								ref={input}
								value={composer}
								onChangeText={setComposer}
								selection={forcedSelection}
								onSelectionChange={(event) => {
									setSelection(event.nativeEvent.selection);
									setForcedSelection(undefined);
								}}
								onFocus={() => setComposerFocused(true)}
								onBlur={() => setComposerFocused(false)}
								placeholder={running ? "O agente está trabalhando…" : "Mensagem para o agente"}
								placeholderTextColor={color.faint}
								style={styles.input}
								multiline
								// Prompts carry identifiers and commands; autocorrect mangles them.
								autoCorrect={false}
								editable={!sending && !dictation.listening}
								accessibilityLabel="Mensagem para o agente"
							/>
							<MicButton dictation={dictation} disabled={sending} />
							{running ? (
								<Pressable
									onPress={() => {
									haptic("warning");
									void stop();
								}}
									style={[styles.sendButton, styles.stopButton]}
									accessibilityRole="button"
									accessibilityLabel="Parar o agente"
								>
									{stopping ? <ActivityIndicator size="small" color={color.danger} /> : <Feather name="square" size={16} color={color.danger} />}
								</Pressable>
							) : (
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
							)}
						</View>
					</Glass>
				</View>
			</View>
		</KeyboardAvoidingView>
	);
}

/** The robot and model label over each agent reply, like the desktop's assistant header. */
function AgentHeader({
	meta,
	label,
	state,
}: {
	meta: SessionSearchResult | null;
	label: string | null;
	state: "idle" | "working" | "waiting";
}) {
	const styles = useStyles();
	return (
		<View style={styles.agentHeader}>
			<AgentMascot provider={meta?.providerId} model={meta?.model} state={state} size={24} />
			{label ? (
				<Mono style={styles.agentLabel} numberOfLines={1}>
					{label}
				</Mono>
			) : null}
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		root: { flex: 1, backgroundColor: color.bg },
		frame: { flex: 1 },
		header: { position: "absolute", left: 16, right: 16, flexDirection: "row", alignItems: "center", gap: 10 },
		titleBlock: { flex: 1, minWidth: 0, alignItems: "center" },
		title: { color: color.fg, fontSize: 16, fontWeight: "600", letterSpacing: -0.2, maxWidth: "100%" },
		subtitle: { flexDirection: "row", alignItems: "center", gap: 6, marginTop: 3, maxWidth: "100%" },
		subtitleText: { fontSize: 11, color: color.mute, flexShrink: 1 },
		diffRow: { position: "absolute", left: 16, right: 16, alignItems: "center" },
		diffPill: { gap: 6, minHeight: 34, paddingHorizontal: 12, maxWidth: "100%" },
		diffBranch: { fontSize: 12, color: color.fg, flexShrink: 1 },
		diffText: { fontSize: 12, color: color.mute },
		error: { color: color.danger, fontSize: 12 },
		notice: { color: color.mute, fontSize: 12 },
		listContent: { paddingHorizontal: 16 },
		agentHeader: { flexDirection: "row", alignItems: "center", gap: 8 },
		agentLabel: { fontSize: 11, color: color.mute, flexShrink: 1 },
		dock: { position: "absolute", left: 12, right: 12, gap: 8 },
		capsule: { paddingHorizontal: 8, paddingVertical: 8, gap: 8 },
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
		stopButton: { backgroundColor: alpha(color.danger, 0.14) },
	}),
);
