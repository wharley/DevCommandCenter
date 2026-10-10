/** Rows shared by both tabs: an agent session and a "needs you" card. */
import {
	formatRelative,
	normalizeProvider,
	sessionTitle,
	workspaceLabel,
	type LiveStatus,
	type PendingItem,
	type SessionSearchResult,
} from "@dcc/mobile-core";
import Feather from "@expo/vector-icons/Feather";
import { router } from "expo-router";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { alpha, color, radius } from "../theme";
import { Button, Mono } from "../ui/primitives";
import { AgentMascot } from "../ui/sprites";

const STATUS_TEXT: Record<LiveStatus, { text: string; tint: string }> = {
	running: { text: "rodando", tint: color.accent },
	completed: { text: "concluído", tint: color.mute },
	aborted: { text: "interrompido", tint: color.danger },
};

export function openThread(sessionId: string) {
	router.push({ pathname: "/thread/[id]", params: { id: sessionId } });
}

/** "Nova tarefa", optionally with the project already picked. */
export function openNewTask(repositoryId?: string | null) {
	router.push(repositoryId ? { pathname: "/new-task", params: { repositoryId } } : "/new-task");
}

export function SessionRow({ session, status }: { session: SessionSearchResult; status?: LiveStatus }) {
	const place = workspaceLabel(session);
	const label = status ? STATUS_TEXT[status] : null;
	return (
		<Pressable
			onPress={() => openThread(session.sessionId)}
			style={({ pressed }) => [styles.row, pressed && { backgroundColor: color.elevated }]}
			accessibilityRole="button"
			accessibilityLabel={`${sessionTitle(session)}${label ? `, ${label.text}` : ""}`}
		>
			<AgentMascot provider={session.providerId} model={session.model} state={status === "running" ? "working" : "idle"} size={28} />
			<View style={{ flex: 1, minWidth: 0, gap: 3 }}>
				<View style={styles.titleLine}>
					<Text style={styles.title} numberOfLines={1}>
						{sessionTitle(session)}
					</Text>
					{session.providerId ? <ProviderTag providerId={session.providerId} /> : null}
				</View>
				{place ? (
					<Mono style={styles.place} numberOfLines={1}>
						{place}
					</Mono>
				) : null}
				<Mono style={styles.meta}>
					{label ? <Text style={{ color: label.tint }}>{label.text} · </Text> : null}
					{formatRelative(session.updatedAt)}
				</Mono>
			</View>
			<Feather name="chevron-right" size={16} color={color.faint} />
		</Pressable>
	);
}

export function ProviderTag({ providerId }: { providerId: string }) {
	return (
		<View style={styles.tag}>
			<Mono style={styles.tagText}>{normalizeProvider(providerId)}</Mono>
		</View>
	);
}

export function NeedsYouCard({
	item,
	resolving,
	onRespond,
}: {
	item: PendingItem;
	resolving: boolean;
	onRespond: (choice: string) => void;
}) {
	const place = workspaceLabel(item.thread);
	return (
		<View style={styles.card}>
			<Pressable onPress={() => openThread(item.thread.sessionId)} style={styles.cardHead} accessibilityRole="button">
				<AgentMascot provider={item.thread.providerId} model={item.thread.model} state="waiting" size={28} />
				<View style={{ flex: 1, minWidth: 0 }}>
					<Text style={styles.title} numberOfLines={1}>
						{sessionTitle(item.thread)}
					</Text>
					{place ? (
						<Mono style={styles.place} numberOfLines={1}>
							{place}
						</Mono>
					) : null}
				</View>
				<Feather name="chevron-right" size={16} color={color.faint} />
			</Pressable>
			<Mono style={styles.question} numberOfLines={6}>
				{item.question}
			</Mono>
			<View style={styles.choices}>
				{item.choices.length === 0 ? (
					<Button label="Responder ao agente" variant="wait" style={{ flex: 1 }} onPress={() => openThread(item.thread.sessionId)} />
				) : (
					item.choices.map((choice) => (
						<Button
							key={choice.id}
							label={choice.label}
							variant={choice.id === "deny" ? "ghost" : "wait"}
							busy={resolving && choice.id !== "deny"}
							disabled={resolving}
							style={{ flex: 1 }}
							onPress={() => onRespond(choice.id)}
						/>
					))
				)}
			</View>
		</View>
	);
}

const styles = StyleSheet.create({
	row: {
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
	titleLine: { flexDirection: "row", alignItems: "center", gap: 6 },
	title: { color: color.fg, fontSize: 14, fontWeight: "500", flexShrink: 1 },
	place: { color: color.faint, fontSize: 11 },
	meta: { color: color.faint, fontSize: 11 },
	tag: { borderWidth: 1, borderColor: color.border, backgroundColor: color.bg, borderRadius: 4, paddingHorizontal: 4, paddingVertical: 1 },
	tagText: { fontSize: 9, color: color.faint },
	card: {
		borderWidth: 1,
		borderColor: alpha(color.wait, 0.4),
		backgroundColor: alpha(color.wait, 0.05),
		borderRadius: radius.lg,
		padding: 14,
		gap: 10,
	},
	cardHead: { flexDirection: "row", alignItems: "center", gap: 10 },
	question: { color: color.fg, fontSize: 12, lineHeight: 18 },
	choices: { flexDirection: "row", gap: 8 },
});
