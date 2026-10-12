import { searchSessions, triageGroups, triageWithAttention, type NeedsYouRow } from "@dcc/mobile-core";
import { useAgentRequests } from "@/backend/agentRequests";
import { HomeHeader } from "@/screens/HomeHeader";
import { pendingKey, useHomeData } from "@/screens/homeData";
import { AgentRequestHeader, AttentionCard, NeedsYouCard, SessionRow } from "@/screens/rows";
import { AgentRequestCard } from "@/ui/AgentRequestCard";
import { color } from "@/theme";
import { EmptyState, Loading, SectionLabel } from "@/ui/primitives";
import { useMemo, useState } from "react";
import { RefreshControl, ScrollView, StyleSheet, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

/** Agentes: triage first — what needs you, what is running, what is recent. */
export default function AgentsScreen() {
	const insets = useSafeAreaInsets();
	const { scan, attention, refreshing, refresh, respond, resolving, showNotice } = useHomeData();
	const agentRequests = useAgentRequests();
	const [search, setSearch] = useState("");
	const groups = useMemo(() => {
		if (!scan) return null;
		const waiting = new Set(agentRequests.requests.map((r) => r.sessionId));
		if (attention) return triageWithAttention(scan, attention, waiting);
		// A desktop without the attention route: permissions and questions from the scan.
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
			running: base.running.filter((s) => !waiting.has(s.sessionId)),
			recent: base.recent.filter((s) => !waiting.has(s.sessionId)),
		};
	}, [scan, attention, agentRequests.requests]);
	const needsYouCount = (groups?.needsYou.length ?? 0) + agentRequests.requests.length;
	const results = useMemo(() => (scan ? searchSessions(scan.sessions, search) : null), [scan, search]);

	return (
		<ScrollView
			style={styles.root}
			contentContainerStyle={[styles.content, { paddingTop: insets.top + 12 }]}
			keyboardShouldPersistTaps="handled"
			refreshControl={<RefreshControl refreshing={refreshing && !!scan} onRefresh={() => void refresh()} tintColor={color.mute} />}
		>
			<HomeHeader search={search} onSearch={setSearch} placeholder="Buscar agentes…" />
			{!groups ? (
				<Loading />
			) : results ? (
				results.length === 0 ? (
					<EmptyState title="Nenhum resultado">Tente outro termo.</EmptyState>
				) : (
					<View style={styles.list}>
						{results.map((s) => (
							<SessionRow key={s.sessionId} session={s} status={scan?.statuses.get(s.sessionId)} />
						))}
					</View>
				)
			) : needsYouCount + groups.running.length + groups.recent.length === 0 ? (
				<EmptyState title="Nada por aqui">Quando um agente trabalhar no computador, ele aparece aqui.</EmptyState>
			) : (
				<View style={{ gap: 26 }}>
					{needsYouCount > 0 ? (
						<View>
							<SectionLabel tone="wait" count={needsYouCount}>
								Precisa de você
							</SectionLabel>
							<View style={styles.list}>
								{agentRequests.requests.map((request) => (
									<AgentRequestCard
										key={request.requestId}
										request={request}
										busy={agentRequests.answering.has(request.requestId)}
										header={
											<AgentRequestHeader
												sessionId={request.sessionId}
												thread={scan?.sessions.find((s) => s.sessionId === request.sessionId) ?? null}
											/>
										}
										onRespond={(answer) =>
											void agentRequests.respond(request, answer).then((notice) => notice && showNotice(notice))
										}
									/>
								))}
								{groups.needsYou.map((row) =>
									row.pending ? (
										<NeedsYouCard
											key={pendingKey(row.pending)}
											item={row.pending}
											resolving={resolving.has(pendingKey(row.pending))}
											onRespond={(choice) => void respond(row.pending!, choice)}
										/>
									) : (
										<AttentionCard key={`${row.entry.workspaceId}/${row.entry.reason}`} row={row} />
									),
								)}
							</View>
						</View>
					) : null}
					{groups.running.length > 0 ? (
						<View>
							<SectionLabel tone="live" count={groups.running.length}>
								Rodando
							</SectionLabel>
							<View style={styles.list}>
								{groups.running.map((s) => (
									<SessionRow key={s.sessionId} session={s} status="running" />
								))}
							</View>
						</View>
					) : null}
					{groups.recent.length > 0 ? (
						<View>
							<SectionLabel>Recentes</SectionLabel>
							<View style={styles.list}>
								{groups.recent.map((s) => (
									<SessionRow key={s.sessionId} session={s} status={scan?.statuses.get(s.sessionId)} />
								))}
							</View>
						</View>
					) : null}
				</View>
			)}
		</ScrollView>
	);
}

const styles = StyleSheet.create({
	root: { flex: 1, backgroundColor: color.bg },
	content: { paddingHorizontal: 16, paddingBottom: 32 },
	list: { gap: 8 },
});
