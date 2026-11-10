/**
 * A room's pieces (`app/room.tsx`), in the desktop's own drawing: a pill at
 * each new day; what the person said as a bubble to the right, each
 * `@Agent` a tinted pill, with who took it; an agent's answer as a flat row
 * — mascot, bold name and the time on one line, the words under, answers
 * close together sharing one face — with its receipt and "Abrir conversa";
 * who is working; the approvals docked on the field; and the team's list
 * that `@` opens.
 */
import {
	approvalTitle,
	handoffHeading,
	pathBasename,
	residentStateLabel,
	roomClock,
	roomReceiptLabel,
	roomStepsLabel,
	roomTaskStateLabel,
	roomWakeLabel,
	splitMentions,
	splitPrompt,
	type ResidentAgent,
	type RoomApproval,
	type RoomLine,
	type RoomTask,
} from "@dcc/mobile-core";
import { alpha, hairline, makeStyles, radius, useTheme } from "@/theme";
import { haptic } from "@/ui/haptics";
import { Button, Mono } from "@/ui/primitives";
import { ResidentMascot } from "@/ui/ResidentMascot";
import { RichText } from "@/ui/RichText";
import Feather from "@expo/vector-icons/Feather";
import { Pressable, StyleSheet, Text, View } from "react-native";

/** The face's side, and so the column the words line up in. */
const FACE = 30;
/** At most this many approvals show; the rest are counted. */
const DOCKED = 3;

export function DayPill({ label }: { label: string }) {
	const styles = useStyles();
	return (
		<View style={styles.day} accessibilityRole="header">
			<Text style={styles.dayText}>{label}</Text>
		</View>
	);
}

export function PersonLine({ line, agent, team }: { line: RoomLine; agent: ResidentAgent | undefined; team: ResidentAgent[] }) {
	const styles = useStyles();
	const passed = handoffHeading(line.text);
	if (passed) {
		return (
			<View style={styles.passed} accessible accessibilityLabel={passed}>
				<Feather name="corner-down-right" size={13} style={styles.passedIcon} />
				<Text style={styles.passedText} numberOfLines={2}>
					{passed.replace(/^↪ /, "")}
				</Text>
			</View>
		);
	}
	if (line.wake) {
		const label = roomWakeLabel(line.wake);
		return (
			<View style={styles.passed} accessible accessibilityLabel={label}>
				<Feather name={line.wake.kind === "source" ? "rss" : "clock"} size={12} style={styles.passedIcon} />
				<Text style={styles.passedText} numberOfLines={2}>
					{label}
					{agent ? ` → ${agent.name}` : ""}
				</Text>
			</View>
		);
	}
	const said = splitPrompt(line.text);
	return (
		<View style={styles.personWrap}>
			<View style={styles.personBubble}>
				{said.attachments.map((path) => (
					<View key={path} style={styles.attachment}>
						<Feather name="paperclip" size={12} style={styles.passedIcon} />
						<Mono style={styles.attachmentText} numberOfLines={1}>
							{pathBasename(path)}
						</Mono>
					</View>
				))}
				{said.text ? (
					<Text style={styles.personText} selectable>
						{splitMentions(said.text, team).map((part, index) =>
							part.agentId ? (
								<Text key={index} style={styles.mention}>
									{part.text}
								</Text>
							) : (
								part.text
							),
						)}
					</Text>
				) : null}
			</View>
			{agent ? <Mono style={styles.routed}>→ {agent.name}</Mono> : null}
		</View>
	);
}

export function AgentLine({
	line,
	agent,
	first,
	onOpen,
	onTask,
}: {
	line: RoomLine;
	agent: ResidentAgent;
	/** Opens a group: the face, the name and the time show. */
	first: boolean;
	onOpen: () => void;
	onTask: (task: RoomTask, where: "thread" | "delivery") => void;
}) {
	const styles = useStyles();
	const receipt = roomReceiptLabel(line.receipt);
	return (
		<View style={styles.agentRow}>
			<View style={styles.face}>{first ? <ResidentMascot avatar={agent.avatar} state="watching" size={FACE} /> : null}</View>
			<View style={styles.agentBody}>
				{first ? (
					<View style={styles.agentHead}>
						<Text style={styles.agentName} numberOfLines={1}>
							{agent.name}
						</Text>
						<Mono style={styles.clock}>{roomClock(line.at)}</Mono>
					</View>
				) : null}
				{line.steps ? (
					<Text style={styles.steps} numberOfLines={2}>
						{roomStepsLabel(line.steps)}
					</Text>
				) : null}
				<RichText text={line.text} />
				{line.tasks.map((task) => (
					<TaskCard key={task.workspaceId} task={task} onOpen={(where) => onTask(task, where)} />
				))}
				<View style={styles.receipt}>
					{receipt ? <Mono style={styles.receiptText}>{receipt}</Mono> : null}
					<Pressable
						onPress={() => {
							haptic("tap");
							onOpen();
						}}
						accessibilityRole="button"
						accessibilityLabel={`Abrir a conversa com ${agent.name}`}
						hitSlop={8}
					>
						<Text style={styles.open}>Abrir conversa</Text>
					</Pressable>
				</View>
			</View>
		</View>
	);
}

/** A task the answer reports: its name, how it stands, what it changed, and where to go. */
function TaskCard({ task, onOpen }: { task: RoomTask; onOpen: (where: "thread" | "delivery") => void }) {
	const { color } = useTheme();
	const styles = useStyles();
	const tint = task.state === "failed" ? color.danger : task.state === "working" ? color.accent : color.mute;
	const press = (where: "thread" | "delivery") => () => {
		haptic("tap");
		onOpen(where);
	};
	return (
		<View style={styles.task}>
			<View style={styles.taskHead}>
				<Feather name="git-branch" size={14} color={color.mute} />
				<Text style={styles.taskTitle} numberOfLines={1}>
					{task.title}
				</Text>
			</View>
			<Mono style={styles.taskMeta} numberOfLines={1}>
				<Text style={{ color: tint }}>{roomTaskStateLabel(task.state)}</Text>
				{task.files > 0 ? ` · ${task.files} ${task.files === 1 ? "arquivo" : "arquivos"} ` : ""}
				{task.files > 0 ? <Text style={{ color: color.accent }}>+{task.additions}</Text> : null}
				{task.files > 0 ? " " : ""}
				{task.files > 0 ? <Text style={{ color: color.danger }}>−{task.deletions}</Text> : null}
			</Mono>
			<View style={styles.taskActions}>
				{task.sessionId ? (
					<Pressable onPress={press("thread")} accessibilityRole="button" accessibilityLabel={`Abrir a tarefa ${task.title}`} style={styles.taskButton}>
						<Text style={styles.taskButtonText}>Abrir tarefa</Text>
					</Pressable>
				) : null}
				<Pressable onPress={press("delivery")} accessibilityRole="button" accessibilityLabel={`Ver as mudanças de ${task.title}`} style={styles.taskButton}>
					<Text style={styles.taskButtonText}>Ver mudanças</Text>
				</Pressable>
			</View>
		</View>
	);
}

/** "Dev está trabalhando…" while an agent works in the room. */
export function WorkingLine({ agent }: { agent: ResidentAgent }) {
	const styles = useStyles();
	return (
		<View style={styles.agentRow} accessibilityLabel={`${agent.name} está trabalhando`}>
			<View style={styles.face}>
				<ResidentMascot avatar={agent.avatar} state="working" size={FACE - 6} />
			</View>
			<Text style={styles.workingText} numberOfLines={1}>
				{agent.name} está trabalhando…
			</Text>
		</View>
	);
}

/** The approvals waiting in the room, right on the field: three at most, then "+N aguardando". */
export function ApprovalDock({
	approvals,
	answering,
	onAnswer,
}: {
	approvals: RoomApproval[];
	answering: ReadonlySet<string>;
	onAnswer: (approval: RoomApproval, allow: boolean) => void;
}) {
	const styles = useStyles();
	if (approvals.length === 0) return null;
	const more = approvals.length - DOCKED;
	return (
		<View style={styles.dock}>
			{approvals.slice(0, DOCKED).map((approval, index) => {
				const busy = answering.has(approval.requestId);
				return (
					<View key={approval.requestId} style={[styles.approval, index > 0 && styles.approvalNext]}>
						<Text style={styles.approvalTitle} numberOfLines={1}>
							{approvalTitle(approval)}
						</Text>
						{approval.summary ? (
							<Mono style={styles.approvalSummary} numberOfLines={2}>
								{approval.summary}
							</Mono>
						) : null}
						<View style={styles.approvalActions}>
							<Button label="Permitir" variant="wait" style={styles.approvalButton} busy={busy} disabled={busy} onPress={() => onAnswer(approval, true)} />
							<Button label="Negar" variant="ghost" style={styles.approvalButton} disabled={busy} onPress={() => onAnswer(approval, false)} />
						</View>
					</View>
				);
			})}
			{more > 0 ? <Text style={styles.approvalMore}>+{more} aguardando</Text> : null}
		</View>
	);
}

/** The team that `@` opens above the field: a tap addresses that agent. */
export function MentionList({ agents, onPick }: { agents: ResidentAgent[]; onPick: (agent: ResidentAgent) => void }) {
	const { color } = useTheme();
	const styles = useStyles();
	if (agents.length === 0) return null;
	return (
		<View style={styles.mentions} accessibilityRole="menu">
			{agents.slice(0, 6).map((agent, index) => (
				<Pressable
					key={agent.id}
					onPress={() => {
						haptic("select");
						onPick(agent);
					}}
					accessibilityRole="menuitem"
					accessibilityLabel={`Chamar ${agent.name}`}
					style={({ pressed }) => [styles.mentionRow, index > 0 && styles.mentionNext, pressed && { backgroundColor: color.elevated }]}
				>
					<ResidentMascot avatar={agent.avatar} state={agent.state} size={24} />
					<Text style={styles.mentionName} numberOfLines={1}>
						{agent.name}
					</Text>
					<Text style={styles.mentionState} numberOfLines={1}>
						{residentStateLabel(agent.state)}
					</Text>
				</Pressable>
			))}
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		day: { alignSelf: "center", paddingHorizontal: 10, paddingVertical: 3, borderRadius: 999, backgroundColor: color.chip, marginVertical: 4 },
		dayText: { color: color.mute, fontSize: 11, fontWeight: "500" },
		personWrap: { alignItems: "flex-end", gap: 4 },
		personBubble: {
			maxWidth: "86%",
			backgroundColor: color.bubble,
			borderRadius: 18,
			borderBottomRightRadius: 6,
			paddingHorizontal: 14,
			paddingVertical: 10,
			gap: 6,
		},
		personText: { color: color.fg, fontSize: 15, lineHeight: 21 },
		mention: { color: color.info, fontWeight: "600", backgroundColor: alpha(color.info, 0.14) },
		attachment: { flexDirection: "row", alignItems: "center", gap: 6 },
		attachmentText: { color: color.mute, fontSize: 12, flexShrink: 1 },
		routed: { fontSize: 11, color: color.faint },
		passed: { flexDirection: "row", alignItems: "center", gap: 6, alignSelf: "center", maxWidth: "92%" },
		passedIcon: { color: color.mute },
		passedText: { color: color.mute, fontSize: 12, flexShrink: 1 },
		agentRow: { flexDirection: "row", alignItems: "flex-start", gap: 10 },
		face: { width: FACE, alignItems: "center" },
		agentBody: { flex: 1, minWidth: 0, gap: 6 },
		agentHead: { flexDirection: "row", alignItems: "baseline", gap: 8 },
		agentName: { color: color.fg, fontSize: 14, fontWeight: "700", flexShrink: 1 },
		clock: { fontSize: 11, color: color.faint },
		steps: { color: color.mute, fontSize: 12, lineHeight: 17 },
		task: { gap: 6, padding: 12, borderRadius: radius.lg, borderWidth: 1, borderColor: color.border, backgroundColor: color.panel },
		taskHead: { flexDirection: "row", alignItems: "center", gap: 8 },
		taskTitle: { flex: 1, color: color.fg, fontSize: 14, fontWeight: "600" },
		taskMeta: { color: color.mute, fontSize: 12 },
		taskActions: { flexDirection: "row", gap: 8, marginTop: 2 },
		taskButton: { minHeight: 36, paddingHorizontal: 14, borderRadius: 18, alignItems: "center", justifyContent: "center", backgroundColor: color.chip },
		taskButtonText: { color: color.fg, fontSize: 13, fontWeight: "500" },
		receipt: { flexDirection: "row", alignItems: "center", gap: 12 },
		receiptText: { fontSize: 11, color: color.faint },
		open: { fontSize: 12, color: color.mute, fontWeight: "500" },
		workingText: { flex: 1, color: color.accent, fontSize: 13, fontWeight: "500", lineHeight: FACE - 6 },
		dock: {
			borderRadius: radius.xl,
			borderWidth: 1,
			borderColor: alpha(color.waitFill, 0.45),
			backgroundColor: alpha(color.waitFill, 0.1),
			overflow: "hidden",
		},
		approval: { paddingHorizontal: 14, paddingVertical: 12, gap: 6 },
		approvalNext: { borderTopWidth: hairline, borderTopColor: alpha(color.waitFill, 0.35) },
		approvalTitle: { color: color.wait, fontSize: 14, fontWeight: "600" },
		approvalSummary: { color: color.fg, fontSize: 12, lineHeight: 17 },
		approvalActions: { flexDirection: "row", gap: 8, marginTop: 2 },
		approvalButton: { flex: 1, minHeight: 40 },
		approvalMore: { color: color.wait, fontSize: 12, paddingHorizontal: 14, paddingBottom: 10 },
		mentions: { borderRadius: radius.lg, borderWidth: 1, borderColor: color.border, backgroundColor: color.panel, overflow: "hidden" },
		mentionRow: { flexDirection: "row", alignItems: "center", gap: 10, minHeight: 44, paddingHorizontal: 12 },
		mentionNext: { borderTopWidth: hairline, borderTopColor: color.border },
		mentionName: { color: color.fg, fontSize: 15, fontWeight: "600", flexShrink: 1 },
		mentionState: { flex: 1, textAlign: "right", color: color.mute, fontSize: 12 },
	}),
);
