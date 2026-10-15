import { latestSessionByWorkspace, projectGroups, searchSessions, sessionTitle, type Comb, type ProjectGroup } from "@dcc/mobile-core";
import { useHomeData } from "@/screens/homeData";
import { useHomeScope } from "@/screens/homeScope";
import { openNewTask, openThread, RecentRow } from "@/screens/rows";
import { font, makeStyles, useTheme } from "@/theme";
import { haptic } from "@/ui/haptics";
import { Loading, SectionLabel, StateDot, type AgentState } from "@/ui/primitives";
import { ProjectGlyph } from "@/ui/sprites";
import { GroupedList, Row, SheetHeader, SheetScroll } from "@/ui/surfaces";
import Feather from "@expo/vector-icons/Feather";
import { router } from "expo-router";
import { useMemo, useState } from "react";
import { StyleSheet, Text, TextInput, View } from "react-native";

/**
 * The project picker: every project with what needs you and what runs
 * there; the picked one opens to its worktrees. What used to be the
 * Workspaces tab, one tap from the home instead of a tab away.
 */
export default function ScopeSheet() {
	const { color } = useTheme();
	const styles = useStyles();
	const { scan, attention, combs, catalog } = useHomeData();
	const { project: current, setScope } = useHomeScope();
	const [query, setQuery] = useState("");
	const sessions = useMemo(() => scan?.sessions ?? [], [scan]);
	const latest = useMemo(() => latestSessionByWorkspace(sessions), [sessions]);
	const projects = useMemo(
		() => (combs ? projectGroups(catalog?.repositories ?? [], combs, sessions, query) : null),
		[catalog, combs, sessions, query],
	);
	const tasks = useMemo(() => searchSessions(sessions, query), [sessions, query]);

	const waiting = useMemo(
		() => new Set(attention ? attention.map((e) => e.workspaceId) : (scan?.needsYou.map((p) => p.thread.workspaceId) ?? [])),
		[attention, scan],
	);
	const stateOf = (comb: Comb): AgentState => {
		if (waiting.has(comb.id)) return "wait";
		const session = latest.get(comb.id);
		return session && scan?.statuses.get(session.sessionId) === "running" ? "live" : "idle";
	};

	const pick = (key: string | null) => {
		haptic("select");
		setScope(key);
		router.back();
	};
	// Leave the sheet first: the thread opens in the stack under it.
	const open = (sessionId: string) => {
		router.back();
		requestAnimationFrame(() => openThread(sessionId));
	};
	const startIn = (repositoryId: string) => {
		router.back();
		requestAnimationFrame(() => openNewTask(repositoryId));
	};

	return (
		<SheetScroll
			contentStyle={styles.content}
			header={
				<View>
					<SheetHeader title="Projetos" onClose={() => router.back()} />
					<View style={styles.search}>
						<Feather name="search" size={15} color={color.mute} />
						<TextInput
							value={query}
							onChangeText={setQuery}
							placeholder="Buscar projetos, worktrees e tarefas"
							placeholderTextColor={color.faint}
							style={styles.searchInput}
							autoCorrect={false}
							clearButtonMode="while-editing"
							accessibilityLabel="Buscar projetos, worktrees e tarefas"
						/>
					</View>
				</View>
			}
		>
			{!projects ? (
				<Loading />
			) : (
				<>
					{query.trim() ? null : (
						<GroupedList>
							<Row
								onPress={() => pick(null)}
								leading={
									<View style={styles.allGlyph}>
										<Feather name="layers" size={15} color={color.mute} />
									</View>
								}
								title="Todos os projetos"
								trailing={
									current ? (
										<Text style={styles.count}>{sessions.length} tarefas</Text>
									) : (
										<Feather name="check" size={18} color={color.accent} />
									)
								}
							/>
						</GroupedList>
					)}
					{projects.map((project) => {
						const expanded = project.key === current?.key || !!query.trim();
						return (
							<GroupedList key={project.key} inset={54}>
								<Row
									onPress={() => pick(project.key)}
									accessibilityLabel={`${project.name}${project.key === current?.key ? ", selecionado" : ""}`}
									leading={<ProjectGlyph icon={project.icon} color={project.color} seed={project.rootPath} size={30} active={project.combs.some((c) => stateOf(c) === "live")} />}
									title={project.name}
									detail={<ProjectSummary project={project} stateOf={stateOf} />}
									trailing={project.key === current?.key ? <Feather name="check" size={18} color={color.accent} /> : null}
								/>
								{expanded
									? project.combs.map((comb) => {
											const session = latest.get(comb.id);
											return (
												<Row
													key={comb.id}
													onPress={session ? () => open(session.sessionId) : undefined}
													leading={
														<View style={styles.dotSlot}>
															<StateDot state={stateOf(comb)} size={9} />
														</View>
													}
													title={session ? sessionTitle(session) : "Sem tarefa ainda"}
													detail={comb.branch ?? comb.name ?? comb.id}
													chevron={!!session}
												/>
											);
										})
									: null}
								{expanded && project.repositoryId ? (
									<Row
										onPress={() => startIn(project.repositoryId!)}
										leading={
											<View style={styles.dotSlot}>
												<Feather name="plus" size={16} color={color.accent} />
											</View>
										}
										title="Nova tarefa aqui"
									/>
								) : null}
							</GroupedList>
						);
					})}
					{tasks && tasks.length > 0 ? (
						<View>
							<SectionLabel>Tarefas</SectionLabel>
							<GroupedList inset={56}>
								{tasks.slice(0, 20).map((s) => (
									<RecentRow key={s.sessionId} session={s} status={scan?.statuses.get(s.sessionId)} onPress={() => open(s.sessionId)} />
								))}
							</GroupedList>
						</View>
					) : null}
					{query.trim() && projects.length === 0 && !tasks?.length ? (
						<Text style={styles.none}>Nada encontrado para “{query.trim()}”.</Text>
					) : null}
				</>
			)}
		</SheetScroll>
	);
}

/** "1 precisa de você · 1 rodando", or how quiet the project is. */
function ProjectSummary({ project, stateOf }: { project: ProjectGroup; stateOf: (comb: Comb) => AgentState }) {
	const { color } = useTheme();
	const styles = useStyles();
	const waiting = project.combs.filter((c) => stateOf(c) === "wait").length;
	const running = project.combs.filter((c) => stateOf(c) === "live").length;
	if (waiting + running === 0) {
		return (
			<Text style={styles.summary} numberOfLines={1}>
				{project.combs.length === 0
					? "nenhum worktree ainda"
					: `${project.combs.length} ${project.combs.length === 1 ? "worktree" : "worktrees"}`}
			</Text>
		);
	}
	return (
		<Text style={styles.summary} numberOfLines={1}>
			{waiting > 0 ? (
				<Text style={{ color: color.wait, fontWeight: "600" }}>
					{waiting} {waiting === 1 ? "precisa" : "precisam"} de você
				</Text>
			) : null}
			{waiting > 0 && running > 0 ? " · " : null}
			{running > 0 ? <Text style={{ color: color.accent, fontWeight: "600" }}>{running} rodando</Text> : null}
		</Text>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		content: { paddingHorizontal: 16, gap: 14 },
		search: {
			flexDirection: "row",
			alignItems: "center",
			gap: 8,
			height: 42,
			marginHorizontal: 16,
			marginBottom: 14,
			paddingHorizontal: 12,
			borderRadius: 12,
			backgroundColor: color.chip,
		},
		searchInput: { flex: 1, color: color.fg, fontSize: 15 },
		allGlyph: { width: 30, height: 30, borderRadius: 9, alignItems: "center", justifyContent: "center", backgroundColor: color.chip },
		dotSlot: { width: 30, alignItems: "center" },
		count: { color: color.mute, fontSize: 12, fontFamily: font.mono },
		summary: { color: color.mute, fontSize: 13 },
		none: { color: color.mute, fontSize: 14, textAlign: "center", paddingVertical: 24 },
	}),
);
