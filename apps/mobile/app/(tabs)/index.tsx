import { searchSessions, triageGroups } from "@dcc/mobile-core";
import { HomeHeader } from "@/screens/HomeHeader";
import { pendingKey, useHomeData } from "@/screens/homeData";
import { NeedsYouCard, SessionRow } from "@/screens/rows";
import { color } from "@/theme";
import { EmptyState, Loading, SectionLabel } from "@/ui/primitives";
import { useMemo, useState } from "react";
import { RefreshControl, ScrollView, StyleSheet, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

/** Agentes: triage first — what needs you, what is running, what is recent. */
export default function AgentsScreen() {
	const insets = useSafeAreaInsets();
	const { scan, refreshing, refresh, respond, resolving } = useHomeData();
	const [search, setSearch] = useState("");
	const groups = useMemo(() => (scan ? triageGroups(scan) : null), [scan]);
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
			) : groups.needsYou.length + groups.running.length + groups.recent.length === 0 ? (
				<EmptyState title="Nada por aqui">Quando um agente trabalhar no computador, ele aparece aqui.</EmptyState>
			) : (
				<View style={{ gap: 26 }}>
					{groups.needsYou.length > 0 ? (
						<View>
							<SectionLabel tone="wait" count={groups.needsYou.length}>
								Precisa de você
							</SectionLabel>
							<View style={styles.list}>
								{groups.needsYou.map((item) => (
									<NeedsYouCard
										key={pendingKey(item)}
										item={item}
										resolving={resolving.has(pendingKey(item))}
										onRespond={(choice) => void respond(item, choice)}
									/>
								))}
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
