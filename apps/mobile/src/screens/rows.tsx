/** The home's rows: live and resting tasks, and the "needs you" cards. */
import {
	attentionLabel,
	formatAgo,
	formatRelative,
	normalizeProvider,
	sessionTitle,
	shortModelName,
	workspaceLabel,
	type LiveStatus,
	type NeedsYouRow,
	type PendingItem,
	type SessionSearchResult,
} from "@dcc/mobile-core";
import Feather from "@expo/vector-icons/Feather";
import { router } from "expo-router";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { markWorkspaceSeen } from "../backend/seenResults";
import { alpha, makeStyles, radius, useTheme, type Palette } from "../theme";
import { Button, Mono } from "../ui/primitives";
import { AgentMascot } from "../ui/sprites";
import { Row } from "../ui/surfaces";

function statusText(status: LiveStatus, color: Palette): { text: string; tint: string } {
	switch (status) {
		case "running":
			return { text: "rodando", tint: color.accent };
		case "completed":
			return { text: "concluído", tint: color.mute };
		case "aborted":
			return { text: "interrompido", tint: color.danger };
		// A wait, not an error: "Precisa de você" says when it needs the person.
		case "limited":
			return { text: "parado no limite de uso", tint: color.mute };
	}
}

export function openThread(sessionId: string) {
	router.push({ pathname: "/thread/[id]", params: { id: sessionId } });
}

/** "Nova tarefa", optionally with the project already picked. */
export function openNewTask(repositoryId?: string | null) {
	router.push(repositoryId ? { pathname: "/new-task", params: { repositoryId } } : "/new-task");
}

/** A task with an agent working right now: the robot walks, the line under the title is green. */
export function RunningRow({ session }: { session: SessionSearchResult }) {
	const { color } = useTheme();
	const styles = useStyles();
	const place = workspaceLabel(session);
	return (
		<Row
			onPress={() => openThread(session.sessionId)}
			accessibilityLabel={`${sessionTitle(session)}, rodando`}
			leading={<AgentMascot provider={session.providerId} model={session.model} state="working" size={30} />}
			title={sessionTitle(session)}
			detail={
				<Mono style={[styles.detail, { color: color.accent }]} numberOfLines={1}>
					rodando{place ? ` · ${place}` : ""}
				</Mono>
			}
			trailing={<RowMeta session={session} />}
		/>
	);
}

const DONE_ICON: Record<LiveStatus, keyof typeof Feather.glyphMap> = {
	running: "loader",
	completed: "check",
	aborted: "slash",
	limited: "clock",
};

/** A task at rest: how it ended, where it ran, when. The robot only shows for live work. */
export function RecentRow({
	session,
	status,
	onPress,
}: {
	session: SessionSearchResult;
	status?: LiveStatus;
	/** Defaults to opening the thread. */
	onPress?: () => void;
}) {
	const { color } = useTheme();
	const styles = useStyles();
	const label = status ? statusText(status, color) : null;
	return (
		<Row
			onPress={onPress ?? (() => openThread(session.sessionId))}
			accessibilityLabel={`${sessionTitle(session)}${label ? `, ${label.text}` : ""}`}
			leading={
				<View style={styles.glyph}>
					<Feather name={status ? DONE_ICON[status] : "message-square"} size={13} color={status === "aborted" ? color.danger : color.mute} />
				</View>
			}
			title={sessionTitle(session)}
			detail={workspaceLabel(session) || undefined}
			trailing={<RowMeta session={session} />}
		/>
	);
}

function RowMeta({ session }: { session: SessionSearchResult }) {
	const styles = useStyles();
	const model = shortModelName(session.model) ?? (session.providerId ? normalizeProvider(session.providerId) : null);
	return (
		<View style={styles.trail}>
			<Mono style={styles.metaText}>{formatAgo(session.updatedAt)}</Mono>
			{model ? (
				<Mono style={styles.metaText} numberOfLines={1}>
					{model}
				</Mono>
			) : null}
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
	const { color } = useTheme();
	const styles = useStyles();
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

const ATTENTION_ICON: Record<NeedsYouRow["entry"]["reason"], keyof typeof Feather.glyphMap> = {
	permission: "shield",
	input: "message-circle",
	usageLimit: "clock",
	conflicts: "git-merge",
	prConflicts: "git-pull-request",
	checksFailing: "x-octagon",
	delegatedReview: "users",
	woke: "sun",
	setup: "tool",
	aborted: "slash",
	completed: "check-circle",
};

/** A task under "Precisa de você" whose answer lives in its thread. */
export function AttentionCard({ row }: { row: NeedsYouRow }) {
	const { color } = useTheme();
	const styles = useStyles();
	const { entry, thread } = row;
	const urgent = entry.reason !== "completed" && entry.reason !== "woke";
	const open = () => {
		if (!entry.sessionId) return;
		if (entry.workspaceId) markWorkspaceSeen(entry.workspaceId);
		openThread(entry.sessionId);
	};
	const place = thread ? workspaceLabel(thread) : "";
	return (
		<Pressable
			onPress={open}
			disabled={!entry.sessionId}
			style={({ pressed }) => [styles.row, urgent && styles.urgent, pressed && { backgroundColor: color.elevated }]}
			accessibilityRole="button"
			accessibilityLabel={`${thread ? sessionTitle(thread) : "Tarefa"}, ${attentionLabel(entry)}`}
		>
			<AgentMascot provider={thread?.providerId} model={thread?.model} state={urgent ? "waiting" : "idle"} size={28} />
			<View style={{ flex: 1, minWidth: 0, gap: 3 }}>
				<Text style={styles.title} numberOfLines={1}>
					{thread ? sessionTitle(thread) : "Tarefa"}
				</Text>
				{place ? (
					<Mono style={styles.place} numberOfLines={1}>
						{place}
					</Mono>
				) : null}
				<View style={styles.reasonLine}>
					<Feather name={ATTENTION_ICON[entry.reason]} size={12} color={urgent ? color.wait : color.mute} />
					<Mono style={[styles.meta, { color: urgent ? color.wait : color.mute }]} numberOfLines={1}>
						{attentionLabel(entry)}
						{entry.returnedAt ? ` · ${formatRelative(entry.returnedAt)}` : ""}
					</Mono>
				</View>
			</View>
			<Feather name="chevron-right" size={16} color={color.faint} />
		</Pressable>
	);
}

/** The thread an agent request belongs to, atop its card on the home screen. */
export function AgentRequestHeader({ sessionId, thread }: { sessionId: string; thread: SessionSearchResult | null }) {
	const { color } = useTheme();
	const styles = useStyles();
	const place = thread ? workspaceLabel(thread) : "";
	return (
		<Pressable onPress={() => openThread(sessionId)} style={styles.cardHead} accessibilityRole="button">
			<AgentMascot provider={thread?.providerId} model={thread?.model} state="waiting" size={28} />
			<View style={{ flex: 1, minWidth: 0 }}>
				<Text style={styles.title} numberOfLines={1}>
					{thread ? sessionTitle(thread) : "Sessão"}
				</Text>
				{place ? (
					<Mono style={styles.place} numberOfLines={1}>
						{place}
					</Mono>
				) : null}
			</View>
			<Feather name="chevron-right" size={16} color={color.faint} />
		</Pressable>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
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
		detail: { fontSize: 12 },
		glyph: { width: 30, height: 30, borderRadius: 999, alignItems: "center", justifyContent: "center", backgroundColor: color.chip },
		trail: { alignItems: "flex-end", gap: 3, maxWidth: 96 },
		metaText: { fontSize: 11, color: color.mute },
		title: { color: color.fg, fontSize: 14, fontWeight: "500", flexShrink: 1 },
		place: { color: color.faint, fontSize: 11 },
		meta: { color: color.faint, fontSize: 11 },
		card: {
			borderWidth: 1,
			borderColor: alpha(color.waitFill, 0.4),
			backgroundColor: alpha(color.waitFill, 0.05),
			borderRadius: radius.lg,
			padding: 14,
			gap: 10,
		},
		cardHead: { flexDirection: "row", alignItems: "center", gap: 10 },
		question: { color: color.fg, fontSize: 12, lineHeight: 18 },
		choices: { flexDirection: "row", gap: 8 },
		urgent: { borderColor: alpha(color.waitFill, 0.4) },
		reasonLine: { flexDirection: "row", alignItems: "center", gap: 6 },
	}),
);
