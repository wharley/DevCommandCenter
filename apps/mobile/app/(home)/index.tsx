import {
	inScope,
	scopeTriage,
	sessionTitle,
	triageGroups,
	triageWithAttention,
	type NeedsYouRow,
	type SessionSearchResult,
} from "@dcc/mobile-core";
import { useAgentRequests } from "@/backend/agentRequests";
import { useBackend } from "@/backend/session";
import { useTaskProposals } from "@/backend/taskProposals";
import { useRooms } from "@/backend/rooms";
import { pendingKey, useHomeData } from "@/screens/homeData";
import { loadHomePage, saveHomePage, type HomePage } from "@/screens/homePage";
import { useHomeScope, useProjectDiff } from "@/screens/homeScope";
import { openRoom, RoomsPage } from "@/screens/RoomsPage";
import { AgentRequestHeader, AttentionCard, NeedsYouCard, openNewTask, openThread, RecentRow, RunningRow } from "@/screens/rows";
import { alpha, makeStyles, useTheme } from "@/theme";
import { AgentRequestCard } from "@/ui/AgentRequestCard";
import { ConnectionBanner } from "@/ui/ConnectionBanner";
import { Glass, GlassButton, GlassPill } from "@/ui/glass";
import { haptic } from "@/ui/haptics";
import { EmptyState, Loading, Mono, Panel, SectionLabel, StateDot } from "@/ui/primitives";
import { ProjectGlyph } from "@/ui/sprites";
import { EdgeFade, GroupedList } from "@/ui/surfaces";
import { TaskProposalCard } from "@/ui/TaskProposalCard";
import Feather from "@expo/vector-icons/Feather";
import { router } from "expo-router";
import { useEffect, useMemo, useState } from "react";
import Animated, { LinearTransition } from "react-native-reanimated";
import { Pressable, RefreshControl, ScrollView, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

/**
 * The one home, in two pages under the same "Precisa de você": "Salas" —
 * Geral and each project's room, where the person talks to the team — and
 * "Tarefas" — what is running and what just finished, for every project or
 * the one picked in the scope chip. It opens on the rooms and remembers the
 * page. The dock at the bottom writes to a room or starts a task; over the
 * rooms a pill says what is running.
 */
export default function HomeScreen() {
	const insets = useSafeAreaInsets();
	const { color } = useTheme();
	const styles = useStyles();
	const backend = useBackend();
	const { scan, attention, refreshing, refresh, respond, resolving, showNotice, error, notice, dismissNotice } = useHomeData();
	const { project, workspaces } = useHomeScope();
	const diff = useProjectDiff(project);
	const agentRequests = useAgentRequests();
	const taskProposals = useTaskProposals();
	const rooms = useRooms(backend);
	const { catalog } = useHomeData();
	const [page, setPage] = useState<HomePage>("rooms");
	useEffect(() => {
		void loadHomePage().then(setPage);
	}, []);
	const show = (next: HomePage) => {
		if (next === page) return;
		haptic("select");
		setPage(next);
		saveHomePage(next);
	};

	const groups = useMemo(() => {
		if (!scan) return null;
		const waiting = new Set(agentRequests.requests.map((r) => r.sessionId));
		const all = attention
			? triageWithAttention(scan, attention, waiting)
			: fromScan(scan, waiting);
		return scopeTriage(all, workspaces);
	}, [scan, attention, agentRequests.requests, workspaces]);

	const workspaceOf = useMemo(() => {
		const map = new Map<string, string | null>();
		for (const s of scan?.sessions ?? []) map.set(s.sessionId, s.workspaceId);
		return map;
	}, [scan]);
	const sessionById = useMemo(() => new Map((scan?.sessions ?? []).map((s) => [s.sessionId, s])), [scan]);
	const requests = agentRequests.requests.filter((r) => inScope(workspaceOf.get(r.sessionId), workspaces));
	const proposals = taskProposals.proposals.filter((p) => inScope(workspaceOf.get(p.parentSessionId), workspaces));

	const needsYouCount = (groups?.needsYou.length ?? 0) + requests.length + proposals.length;
	const runningCount = groups?.running.length ?? 0;
	const empty = groups && needsYouCount + runningCount + groups.recent.length === 0;
	// The room the dock writes to: the scoped project's when it has one, else Geral.
	const scopedProjectId = project ? (catalog?.repositories.find((r) => r.id === project.repositoryId)?.projectId ?? null) : null;
	const dockRoom = (scopedProjectId && rooms.rooms?.find((room) => room.id === scopedProjectId)) || { id: null, name: "Geral" };
	const firstRunning = groups?.running[0] ?? null;
	// Over the rooms only the first card shows, so the rooms stay in sight; the rest is one tap away.
	const cap = page === "rooms" ? 1 : Number.POSITIVE_INFINITY;
	const shownRequests = requests.slice(0, cap);
	const shownProposals = proposals.slice(0, Math.max(0, cap - shownRequests.length));
	const shownRows = (groups?.needsYou ?? []).slice(0, Math.max(0, cap - shownRequests.length - shownProposals.length));
	const hiddenNeedsYou = needsYouCount - shownRequests.length - shownProposals.length - shownRows.length;
	const topSpace = insets.top + 64;
	const bottomSpace = insets.bottom + 92;

	return (
		<View style={styles.root}>
			<ScrollView
				contentContainerStyle={[styles.content, { paddingTop: topSpace, paddingBottom: bottomSpace + 24 }]}
				keyboardShouldPersistTaps="handled"
				scrollIndicatorInsets={{ top: topSpace - insets.top, bottom: bottomSpace - insets.bottom }}
				refreshControl={
					<RefreshControl refreshing={refreshing && !!scan} onRefresh={() => void refresh()} tintColor={color.mute} progressViewOffset={topSpace} />
				}
			>
				<ConnectionBanner backend={backend} />
				{error ? (
					<Panel tone="danger" style={styles.message}>
						<Text style={styles.errorText}>{error}</Text>
					</Panel>
				) : null}
				{notice ? (
					<Pressable onPress={dismissNotice}>
						<Panel style={styles.message}>
							<Text style={styles.noticeText}>{notice}</Text>
						</Panel>
					</Pressable>
				) : null}

				<View style={styles.heading}>
					{diff && page === "tasks" ? <DiffLine diff={diff} /> : null}
					<View style={styles.pages} accessibilityRole="tablist">
						{(["rooms", "tasks"] as const).map((id) => (
							<Pressable
								key={id}
								onPress={() => show(id)}
								accessibilityRole="tab"
								accessibilityState={{ selected: page === id }}
								hitSlop={6}
							>
								<Text style={[styles.title, page !== id && styles.titleOff]}>{id === "rooms" ? "Salas" : "Tarefas"}</Text>
							</Pressable>
						))}
					</View>
					<Summary needsYou={needsYouCount} running={runningCount} loaded={!!groups} />
				</View>

				{!groups ? (
					page === "rooms" ? (
						<RoomsPage state={rooms} repositories={catalog?.repositories ?? []} />
					) : (
						<Loading />
					)
				) : empty && page === "tasks" ? (
					<EmptyState icon="sun" title={project ? `Nada em ${project.name} ainda` : "Nada por aqui"}>
						Peça uma tarefa aqui embaixo, ou comece uma no computador.
					</EmptyState>
				) : (
					<Animated.View layout={LinearTransition.duration(220)} style={styles.sections}>
						{needsYouCount > 0 ? (
							<View>
								<SectionLabel tone="wait" count={needsYouCount}>
									Precisa de você
								</SectionLabel>
								<Animated.View layout={LinearTransition.duration(220)} style={styles.cards}>
									{shownRequests.map((request) => (
										<AgentRequestCard
											key={request.requestId}
											request={request}
											busy={agentRequests.answering.has(request.requestId)}
											header={<AgentRequestHeader sessionId={request.sessionId} thread={sessionById.get(request.sessionId) ?? null} />}
											onRespond={(answer) => {
												haptic("success");
												void agentRequests.respond(request, answer).then((text) => text && showNotice(text));
											}}
										/>
									))}
									{shownProposals.map((proposal) => (
										<TaskProposalCard
											key={proposal.id}
											proposal={proposal}
											busy={taskProposals.answering.has(proposal.id)}
											header={
												<AgentRequestHeader
													sessionId={proposal.parentSessionId}
													thread={sessionById.get(proposal.parentSessionId) ?? null}
												/>
											}
											onAnswer={(action) => {
												haptic("select");
												void taskProposals.answer(proposal, action).then((text) => text && showNotice(text));
											}}
										/>
									))}
									{shownRows.map((row) =>
										row.pending ? (
											<NeedsYouCard
												key={pendingKey(row.pending)}
												item={row.pending}
												resolving={resolving.has(pendingKey(row.pending))}
												onRespond={(choice) => {
													haptic(choice === "deny" ? "warning" : "success");
													void respond(row.pending!, choice);
												}}
											/>
										) : (
											<AttentionCard key={`${row.entry.workspaceId}/${row.entry.reason}`} row={row} />
										),
									)}
									{hiddenNeedsYou > 0 ? (
										<Pressable onPress={() => show("tasks")} accessibilityRole="button" style={styles.moreWaiting}>
											<Text style={styles.moreWaitingText}>
												+{hiddenNeedsYou} {hiddenNeedsYou === 1 ? "precisa" : "precisam"} de você
											</Text>
											<Feather name="chevron-right" size={15} color={color.wait} />
										</Pressable>
									) : null}
								</Animated.View>
							</View>
						) : null}
						{page === "rooms" ? <RoomsPage state={rooms} repositories={catalog?.repositories ?? []} /> : null}
						{page === "tasks" && runningCount > 0 ? (
							<View>
								<SectionLabel tone="live">Rodando</SectionLabel>
								<GroupedList inset={56}>
									{groups.running.map((s) => (
										<RunningRow key={s.sessionId} session={s} />
									))}
								</GroupedList>
							</View>
						) : null}
						{page === "tasks" && groups.recent.length > 0 ? (
							<View>
								<SectionLabel>Recentes</SectionLabel>
								<GroupedList inset={56}>
									{groups.recent.map((s) => (
										<RecentRow key={s.sessionId} session={s} status={scan?.statuses.get(s.sessionId)} />
									))}
								</GroupedList>
							</View>
						) : null}
					</Animated.View>
				)}
			</ScrollView>

			<EdgeFade edge="top" height={topSpace + 28} solid={(insets.top + 50) / (topSpace + 28)} />
			<EdgeFade edge="bottom" height={bottomSpace + 24} solid={0.55} />

			<View style={[styles.topBar, { top: insets.top + 6 }]}>
				<GlassPill
					onPress={() => router.push("/scope")}
					accessibilityLabel={`Projeto: ${project?.name ?? "todos os projetos"}. Trocar`}
					style={styles.scopePill}
				>
					{project ? (
						<ProjectGlyph icon={project.icon} color={project.color} seed={project.rootPath} size={26} />
					) : (
						<View style={styles.allGlyph}>
							<Feather name="layers" size={14} color={color.mute} />
						</View>
					)}
					<Text style={styles.scopeText} numberOfLines={1}>
						{project?.name ?? "Todos os projetos"}
					</Text>
					<Feather name="chevron-down" size={15} color={color.mute} />
				</GlassPill>
				<GlassButton icon="settings" tint={color.mute} accessibilityLabel="Ajustes" onPress={() => router.push("/settings")} />
			</View>

			<View style={[styles.dock, { bottom: insets.bottom + 10 }]}>
				{page === "rooms" && runningCount > 0 ? (
					<GlassPill
						onPress={() => {
							haptic("tap");
							if (runningCount === 1 && firstRunning) openThread(firstRunning.sessionId);
							else show("tasks");
						}}
						accessibilityLabel={`${runningCount} ${runningCount === 1 ? "tarefa rodando" : "tarefas rodando"}. Abrir`}
						style={styles.runningPill}
					>
						<StateDot state="live" size={7} />
						<Text style={styles.runningText} numberOfLines={1}>
							<Text style={styles.runningCount}>{runningCount} rodando</Text>
							{firstRunning ? ` · ${sessionTitle(firstRunning)}` : ""}
						</Text>
						<Feather name="chevron-right" size={15} color={color.mute} />
					</GlassPill>
				) : null}
				<Pressable
					onPress={() => {
						haptic("tap");
						if (page === "rooms") openRoom(dockRoom);
						else openNewTask(project?.repositoryId);
					}}
					accessibilityRole="button"
					accessibilityLabel={page === "rooms" ? `Escrever na sala ${dockRoom.name}` : `Nova tarefa${project ? ` em ${project.name}` : ""}`}
				>
					<Glass interactive style={styles.composer}>
						<View style={styles.plus}>
							<Feather name={page === "rooms" ? "hash" : "plus"} size={18} color={color.fg} />
						</View>
						<Text style={styles.placeholder} numberOfLines={1}>
							{page === "rooms"
								? `Escrever em ${dockRoom.name}…`
								: project
									? `Pedir algo em ${project.name}…`
									: "Pedir uma nova tarefa…"}
						</Text>
						<Feather name="arrow-up-right" size={18} color={color.mute} />
					</Glass>
				</Pressable>
			</View>
		</View>
	);
}

/** "⑂ feat/x · 3 arquivos +42 −8" for one worktree, "2 worktrees · …" for several. */
function DiffLine({ diff }: { diff: NonNullable<ReturnType<typeof useProjectDiff>> }) {
	const { color } = useTheme();
	const styles = useStyles();
	const where = diff.branch ?? `${diff.worktrees} worktrees`;
	return (
		<View style={styles.diffLine} accessibilityLabel={`${where}: ${diff.files} arquivos alterados, ${diff.additions} linhas a mais, ${diff.deletions} a menos`}>
			<Feather name="git-branch" size={12} color={color.mute} />
			<Mono style={styles.diffText} numberOfLines={1}>
				{where} · {diff.files === 0 ? "sem mudanças" : `${diff.files} ${diff.files === 1 ? "arquivo" : "arquivos"}`}
				{diff.files > 0 ? (
					<>
						{" "}
						<Text style={{ color: color.accent }}>+{diff.additions}</Text> <Text style={{ color: color.danger }}>−{diff.deletions}</Text>
					</>
				) : null}
			</Mono>
		</View>
	);
}

/** "2 precisam de você · 1 rodando", or a calm line when nothing waits. */
function Summary({ needsYou, running, loaded }: { needsYou: number; running: number; loaded: boolean }) {
	const { color } = useTheme();
	const styles = useStyles();
	if (!loaded) return null;
	if (needsYou + running === 0) return <Text style={styles.summary}>Nenhum agente esperando você.</Text>;
	return (
		<Text style={styles.summary}>
			{needsYou > 0 ? (
				<Text style={{ color: color.wait, fontWeight: "600" }}>
					{needsYou} {needsYou === 1 ? "precisa" : "precisam"} de você
				</Text>
			) : null}
			{needsYou > 0 && running > 0 ? " · " : null}
			{running > 0 ? <Text style={{ color: color.accent, fontWeight: "600" }}>{running} rodando</Text> : null}
		</Text>
	);
}

/** A desktop without the attention route: permissions and questions from the scan. */
function fromScan(scan: NonNullable<ReturnType<typeof useHomeData>["scan"]>, waiting: ReadonlySet<string>) {
	const base = triageGroups(scan);
	return {
		needsYou: base.needsYou.map(
			(pending): NeedsYouRow => ({
				entry: {
					workspaceId: pending.thread.workspaceId ?? "",
					reason: pending.choices.length > 0 ? "permission" : "input",
					count: null,
					returnedAt: pending.at,
					completedAt: null,
					sessionId: pending.thread.sessionId,
				},
				thread: pending.thread,
				pending,
			}),
		),
		// A thread waiting on a Browser/computer-use answer is not "running".
		running: base.running.filter((s: SessionSearchResult) => !waiting.has(s.sessionId)),
		recent: base.recent.filter((s: SessionSearchResult) => !waiting.has(s.sessionId)),
	};
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		root: { flex: 1, backgroundColor: color.bg },
		content: { paddingHorizontal: 16 },
		message: { marginBottom: 12, paddingVertical: 10 },
		errorText: { color: color.danger, fontSize: 12, lineHeight: 17 },
		noticeText: { color: color.mute, fontSize: 12, lineHeight: 17 },
		heading: { paddingHorizontal: 4, marginTop: 6, marginBottom: 22, gap: 4 },
		pages: { flexDirection: "row", alignItems: "baseline", gap: 18 },
		title: { color: color.fg, fontSize: 32, lineHeight: 38, fontWeight: "700", letterSpacing: -0.8 },
		titleOff: { color: color.faint },
		summary: { color: color.mute, fontSize: 14 },
		diffLine: { flexDirection: "row", alignItems: "center", gap: 6, marginBottom: 2 },
		diffText: { color: color.mute, fontSize: 12, flexShrink: 1 },
		sections: { gap: 26 },
		cards: { gap: 10 },
		moreWaiting: {
			flexDirection: "row",
			alignItems: "center",
			justifyContent: "space-between",
			minHeight: 44,
			paddingHorizontal: 16,
			borderRadius: 16,
			backgroundColor: alpha(color.waitFill, 0.1),
		},
		moreWaitingText: { color: color.wait, fontSize: 14, fontWeight: "600" },
		topBar: { position: "absolute", left: 16, right: 16, flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 12 },
		scopePill: { paddingLeft: 9, paddingRight: 14, maxWidth: 280 },
		scopeText: { color: color.fg, fontSize: 15, fontWeight: "600", letterSpacing: -0.2, flexShrink: 1 },
		allGlyph: { width: 26, height: 26, borderRadius: 8, alignItems: "center", justifyContent: "center", backgroundColor: color.chip },
		dock: { position: "absolute", left: 16, right: 16, gap: 8 },
		runningPill: { alignSelf: "center", maxWidth: "100%", paddingLeft: 14, paddingRight: 10 },
		runningText: { color: color.mute, fontSize: 13, flexShrink: 1 },
		runningCount: { color: color.accent, fontWeight: "600" },
		composer: { flexDirection: "row", alignItems: "center", gap: 12, height: 56, paddingLeft: 8, paddingRight: 18 },
		plus: { width: 40, height: 40, borderRadius: 999, alignItems: "center", justifyContent: "center", backgroundColor: color.chip },
		placeholder: { flex: 1, color: color.mute, fontSize: 15 },
	}),
);
