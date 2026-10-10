import { latestSessionByWorkspace, projectGroups, type Comb } from "@dcc/mobile-core";
import { HomeHeader } from "@/screens/HomeHeader";
import { useHomeData } from "@/screens/homeData";
import { openNewTask, openThread } from "@/screens/rows";
import { color, radius } from "@/theme";
import { EmptyState, Loading, Mono, StateDot } from "@/ui/primitives";
import { ProjectGlyph } from "@/ui/sprites";
import Feather from "@expo/vector-icons/Feather";
import { useMemo, useState } from "react";
import { Pressable, RefreshControl, ScrollView, StyleSheet, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

/**
 * Workspaces: every project on the desktop, with its worktrees — the
 * desktop's mental model. A project with no workspace yet (just added, or
 * every task archived) still shows, with a way to start its first task.
 */
export default function WorkspacesScreen() {
	const insets = useSafeAreaInsets();
	const { scan, combs, catalog, refreshing, refresh } = useHomeData();
	const [search, setSearch] = useState("");
	const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
	const sessions = useMemo(() => scan?.sessions ?? [], [scan]);
	const latest = useMemo(() => latestSessionByWorkspace(sessions), [sessions]);
	const projects = useMemo(
		() => (combs ? projectGroups(catalog?.repositories ?? [], combs, sessions, search) : null),
		[catalog, combs, sessions, search],
	);
	const toggle = (key: string) =>
		setCollapsed((prev) => {
			const next = new Set(prev);
			if (next.has(key)) next.delete(key);
			else next.add(key);
			return next;
		});

	const isRunning = (comb: Comb) => {
		const session = latest.get(comb.id);
		return session ? scan?.statuses.get(session.sessionId) === "running" : false;
	};
	const needsYou = new Set(scan?.needsYou.map((p) => p.thread.workspaceId) ?? []);

	return (
		<ScrollView
			style={styles.root}
			contentContainerStyle={[styles.content, { paddingTop: insets.top + 12 }]}
			keyboardShouldPersistTaps="handled"
			refreshControl={<RefreshControl refreshing={refreshing && !!combs} onRefresh={() => void refresh()} tintColor={color.mute} />}
		>
			<HomeHeader search={search} onSearch={setSearch} placeholder="Buscar projetos e workspaces…" />
			{!projects ? (
				<Loading />
			) : projects.length === 0 ? (
				search.trim() ? (
					<EmptyState title="Nenhum resultado">Tente outro termo.</EmptyState>
				) : (
					<EmptyState title="Nenhum projeto">Adicione um projeto pelo app do computador.</EmptyState>
				)
			) : (
				<View style={{ gap: 18 }}>
					{projects.map((project) => {
						const closed = collapsed.has(project.key);
						const active = project.combs.some(isRunning);
						return (
							<View key={project.key}>
								<Pressable
									onPress={() => toggle(project.key)}
									style={styles.projectHead}
									accessibilityRole="button"
									accessibilityState={{ expanded: !closed }}
								>
									<Feather name={closed ? "chevron-right" : "chevron-down"} size={14} color={color.faint} />
									<ProjectGlyph icon={project.icon} color={project.color} seed={project.rootPath} active={active} size={26} />
									<Text style={styles.project} numberOfLines={1}>
										{project.name}
									</Text>
									<Mono style={styles.count}>{project.combs.length}</Mono>
								</Pressable>
								{closed ? null : (
									<View style={styles.list}>
										{project.combs.map((comb) => {
											const session = latest.get(comb.id);
											const state = needsYou.has(comb.id) ? "wait" : isRunning(comb) ? "live" : "idle";
											return (
												<Pressable
													key={comb.id}
													disabled={!session}
													onPress={() => session && openThread(session.sessionId)}
													style={({ pressed }) => [styles.comb, pressed && { backgroundColor: color.elevated }]}
													accessibilityRole="button"
												>
													<StateDot state={state} />
													<View style={{ flex: 1, minWidth: 0, gap: 2 }}>
														<Text style={styles.combName} numberOfLines={1}>
															{comb.name ?? comb.branch ?? comb.id}
														</Text>
														{comb.branch ? (
															<Mono style={styles.branch} numberOfLines={1}>
																{comb.branch}
															</Mono>
														) : null}
														<Text style={styles.sessionLine} numberOfLines={1}>
															{session ? session.threadTitle ?? "Sessão sem título" : "Sem sessão ainda"}
														</Text>
													</View>
													{session ? <Feather name="chevron-right" size={16} color={color.faint} /> : null}
												</Pressable>
											);
										})}
										{project.repositoryId ? (
											<Pressable
												onPress={() => openNewTask(project.repositoryId)}
												style={({ pressed }) => [styles.newTask, pressed && { backgroundColor: color.elevated }]}
												accessibilityRole="button"
												accessibilityLabel={`Nova tarefa em ${project.name}`}
											>
												<Feather name="plus" size={14} color={color.mute} />
												<Text style={styles.newTaskText}>
													{project.combs.length === 0 ? "Nenhum workspace ainda · Nova tarefa" : "Nova tarefa"}
												</Text>
											</Pressable>
										) : null}
									</View>
								)}
							</View>
						);
					})}
				</View>
			)}
		</ScrollView>
	);
}

const styles = StyleSheet.create({
	root: { flex: 1, backgroundColor: color.bg },
	content: { paddingHorizontal: 16, paddingBottom: 32 },
	projectHead: { flexDirection: "row", alignItems: "center", gap: 8, paddingVertical: 6, marginBottom: 6 },
	project: { color: color.fg, fontSize: 14, fontWeight: "600", flex: 1 },
	count: { color: color.faint, fontSize: 11 },
	list: { gap: 8 },
	comb: {
		flexDirection: "row",
		alignItems: "center",
		gap: 12,
		paddingHorizontal: 14,
		paddingVertical: 12,
		borderRadius: radius.md,
		borderWidth: 1,
		borderColor: color.border,
		backgroundColor: color.panel,
	},
	combName: { color: color.fg, fontSize: 14, fontWeight: "500" },
	branch: { color: color.info, fontSize: 11 },
	sessionLine: { color: color.mute, fontSize: 12 },
	newTask: {
		flexDirection: "row",
		alignItems: "center",
		gap: 8,
		paddingHorizontal: 14,
		paddingVertical: 11,
		borderRadius: radius.md,
		borderWidth: 1,
		borderStyle: "dashed",
		borderColor: color.border,
	},
	newTaskText: { color: color.mute, fontSize: 13 },
});
