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
import { ActionSheetIOS, Alert, Platform, Pressable, StyleSheet, Text, View } from "react-native";
import { useBackend } from "../backend/session";
import { errorMessage } from "../backend/types";
import { haptic } from "../ui/haptics";
import { SwipeRow } from "../ui/SwipeRow";
import { useHomeData } from "./homeData";
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

/** Stop, complete, archive and the delivery sheet, from a row's swipe or menu. */
function useTaskActions() {
	const backend = useBackend();
	const { refresh, showNotice } = useHomeData();
	const fail = (error: unknown, fallback: string) => showNotice(errorMessage(error, fallback));
	const stop = (sessionId: string) => {
		haptic("warning");
		void backend
			.request("POST", `/api/v1/sessions/${encodeURIComponent(sessionId)}/abort`, { sessionId })
			.then(() => refresh(), (error) => fail(error, "Falha ao parar."));
	};
	const workspaceAction = (workspaceId: string, action: "complete" | "archive", done: string) =>
		void backend
			.request("POST", `/api/v1/mobile/workspaces/${encodeURIComponent(workspaceId)}/delivery/${action}`, undefined, { timeoutMs: 180_000 })
			.then(
				() => {
					haptic("success");
					showNotice(done);
					return refresh();
				},
				(error) => fail(error, "A ação falhou."),
			);
	const complete = (workspaceId: string) =>
		Alert.alert("Concluir a tarefa?", "Ela sai das listas de ativas, como no computador.", [
			{ text: "Cancelar", style: "cancel" },
			{ text: "Concluir", onPress: () => workspaceAction(workspaceId, "complete", "Tarefa concluída.") },
		]);
	const archive = (workspaceId: string) =>
		Alert.alert("Arquivar a tarefa?", "O workspace fica arquivado no computador. Mudanças não enviadas continuam na pasta.", [
			{ text: "Cancelar", style: "cancel" },
			{ text: "Arquivar", style: "destructive", onPress: () => workspaceAction(workspaceId, "archive", "Tarefa arquivada.") },
		]);
	const delivery = (workspaceId: string) => router.push({ pathname: "/delivery/[id]", params: { id: workspaceId } });
	return { stop, complete, archive, delivery };
}

type MenuItem = { label: string; destructive?: boolean; run: () => void };

/** The row's long-press menu: the system action sheet on iOS, an alert elsewhere. */
function showMenu(title: string, items: MenuItem[]) {
	if (Platform.OS === "ios") {
		const destructive = items.findIndex((item) => item.destructive);
		ActionSheetIOS.showActionSheetWithOptions(
			{
				title,
				options: [...items.map((item) => item.label), "Cancelar"],
				cancelButtonIndex: items.length,
				...(destructive >= 0 ? { destructiveButtonIndex: destructive } : {}),
			},
			(index) => items[index]?.run(),
		);
		return;
	}
	Alert.alert(title, undefined, [
		...items.map((item) => ({ text: item.label, style: item.destructive ? ("destructive" as const) : ("default" as const), onPress: item.run })),
		{ text: "Cancelar", style: "cancel" as const },
	]);
}

/** A task with an agent working right now: the robot walks, the line under the title is green. */
export function RunningRow({ session }: { session: SessionSearchResult }) {
	const { color } = useTheme();
	const styles = useStyles();
	const actions = useTaskActions();
	const place = workspaceLabel(session);
	const workspaceId = session.workspaceId;
	const menu = () =>
		showMenu(sessionTitle(session), [
			{ label: "Abrir a conversa", run: () => openThread(session.sessionId) },
			...(workspaceId ? [{ label: "Ver mudanças", run: () => actions.delivery(workspaceId) }] : []),
			{ label: "Parar o agente", destructive: true, run: () => actions.stop(session.sessionId) },
		]);
	return (
		<SwipeRow action={{ label: "Parar", icon: "square", tint: color.danger, onPress: () => actions.stop(session.sessionId) }}>
			<Row
				onLongPress={menu}
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
		</SwipeRow>
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
	const actions = useTaskActions();
	const label = status ? statusText(status, color) : null;
	const workspaceId = session.workspaceId;
	const menu = () =>
		showMenu(sessionTitle(session), [
			{ label: "Abrir a conversa", run: () => openThread(session.sessionId) },
			...(workspaceId
				? [
						{ label: "Ver mudanças", run: () => actions.delivery(workspaceId) },
						{ label: "Concluir", run: () => actions.complete(workspaceId) },
						{ label: "Arquivar", destructive: true, run: () => actions.archive(workspaceId) },
					]
				: []),
		]);
	return (
		<SwipeRow
			action={workspaceId ? { label: "Arquivar", icon: "archive", tint: color.waitFill, ink: color.waitInk, onPress: () => actions.archive(workspaceId) } : null}
		>
			<Row
				onLongPress={menu}
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
		</SwipeRow>
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
			<View style={styles.ask}>
				<Mono style={styles.question} numberOfLines={6}>
					{item.question}
				</Mono>
			</View>
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
			borderRadius: radius.xl,
			padding: 14,
			gap: 12,
		},
		// What the agent wants to run, set apart from the card.
		ask: { borderRadius: radius.md, backgroundColor: color.panel, paddingHorizontal: 12, paddingVertical: 10 },
		cardHead: { flexDirection: "row", alignItems: "center", gap: 10 },
		question: { color: color.fg, fontSize: 13, lineHeight: 19 },
		choices: { flexDirection: "row", gap: 8 },
		urgent: { borderColor: alpha(color.waitFill, 0.4) },
		reasonLine: { flexDirection: "row", alignItems: "center", gap: 6 },
	}),
);
