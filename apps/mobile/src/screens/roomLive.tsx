/**
 * The room's turns at work on the phone, as the desktop's live rows draw
 * them (`room_live_turn`, `room_task_live`): the agent's face at work, its
 * name (or "Dev · “Corrigir domínio”" for a task it opened), a clock, what
 * it does now ("Lendo src/cache.rs", "Pensando…", "Escrevendo…"), its last
 * steps and the answer so far, with "Parar" (or "Abrir tarefa"); a turn that
 * stopped with "Tentar de novo"; a message queued with "Na fila · …"; and
 * the "N novas" button that takes the room back to its end.
 */
import {
	liveClock,
	liveLabel,
	liveSeconds,
	liveTitle,
	moreStepsLabel,
	newLinesLabel,
	stepText,
	stoppedLabel,
	t,
	type ResidentAgent,
	type RoomLive,
	type RoomStopped,
} from "@dcc/mobile-core";
import { alpha, makeStyles, useTheme } from "@/theme";
import { haptic } from "@/ui/haptics";
import { Mono } from "@/ui/primitives";
import { ResidentMascot } from "@/ui/ResidentMascot";
import { RichText } from "@/ui/RichText";
import Feather from "@expo/vector-icons/Feather";
import { memo, useEffect, useRef, useState } from "react";
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from "react-native";
import { StepRow } from "./roomParts";

const FACE = 30;
/** The steps listed under an agent at work, and under a task. */
const STEPS_SHOWN = 6;
const TASK_STEPS_SHOWN = 3;
/** The streamed answer is drawn again at most this often. */
const TEXT_EVERY_MS = 700;

/** `text`, taken at most once per `every` ms: a stream redraws its markdown now and then. */
function useThrottled(text: string, every: number): string {
	const [shown, setShown] = useState(text);
	const last = useRef(0);
	useEffect(() => {
		const wait = last.current + every - Date.now();
		if (wait <= 0) {
			last.current = Date.now();
			setShown(text);
			return;
		}
		const timer = setTimeout(() => {
			last.current = Date.now();
			setShown(text);
		}, wait);
		return () => clearTimeout(timer);
	}, [text, every]);
	return shown;
}

/** Seconds since `startedAt`, ticking each second while drawn. */
function useClock(live: RoomLive): number {
	const [now, setNow] = useState(() => Date.now());
	useEffect(() => {
		const tick = setInterval(() => setNow(Date.now()), 1000);
		return () => clearInterval(tick);
	}, []);
	return liveSeconds(live, now);
}

function LiveRowView({
	live,
	agent,
	stopping,
	onStop,
	onOpen,
}: {
	live: RoomLive;
	agent: ResidentAgent;
	stopping: boolean;
	onStop: (live: RoomLive) => void;
	/** Opens the task, or the agent's conversation. */
	onOpen: (live: RoomLive) => void;
}) {
	const { color } = useTheme();
	const styles = useStyles();
	const seconds = useClock(live);
	const text = useThrottled(live.text, TEXT_EVERY_MS);
	const shown = live.kind === "task" ? TASK_STEPS_SHOWN : STEPS_SHOWN;
	const steps = live.steps.slice(-shown);
	const earlier = live.earlier + live.steps.length - steps.length;
	const label = liveLabel(live, seconds);
	const title = liveTitle(live, agent.name);
	return (
		<View style={styles.row} accessibilityLabel={`${title}: ${label}`}>
			<View style={styles.face}>
				<ResidentMascot avatar={agent.avatar} state="working" size={FACE} />
			</View>
			<View style={styles.body}>
				<View style={styles.head}>
					<Text style={styles.name} numberOfLines={1}>
						{title}
					</Text>
					<Mono style={styles.clock}>{liveClock(seconds)}</Mono>
					<View style={{ flex: 1 }} />
					{live.kind === "task" ? (
						<Pressable
							onPress={() => {
								haptic("tap");
								onOpen(live);
							}}
							accessibilityRole="button"
							hitSlop={8}
						>
							<Text style={styles.action}>{t("agents.room.task.open")}</Text>
						</Pressable>
					) : (
						<Pressable
							onPress={() => {
								haptic("warning");
								onStop(live);
							}}
							disabled={stopping}
							accessibilityRole="button"
							accessibilityLabel={`Parar ${agent.name}`}
							hitSlop={8}
							style={styles.stop}
						>
							{stopping ? <ActivityIndicator size="small" color={color.mute} /> : <Feather name="square" size={10} color={color.fg} />}
							<Text style={styles.stopText}>{stopping ? t("agents.room.live.stopping") : t("agents.room.live.stop")}</Text>
						</Pressable>
					)}
				</View>
				<Text style={styles.doing} numberOfLines={1}>
					{label}
				</Text>
				{steps.length > 0 ? (
					<View style={styles.steps}>
						{earlier > 0 ? <Text style={styles.more}>{moreStepsLabel(earlier)}</Text> : null}
						{steps.map((step, index) => (
							<StepRow key={earlier + index} text={stepText(step, step.state === "running")} state={step.state} />
						))}
					</View>
				) : null}
				{live.kind === "conversation" && text.trim() ? <RichText text={text} /> : null}
			</View>
		</View>
	);
}

export const LiveRow = memo(LiveRowView);

/** "Dev parou antes de terminar · por quê" with "Tentar de novo" when the person sent it. */
export const StoppedRow = memo(function StoppedRow({
	stopped,
	agent,
	busy,
	onRetry,
}: {
	stopped: RoomStopped;
	agent: ResidentAgent;
	busy: boolean;
	onRetry: (stopped: RoomStopped) => void;
}) {
	const { color } = useTheme();
	const styles = useStyles();
	return (
		<View style={styles.stopped}>
			<View style={styles.face}>
				<ResidentMascot avatar={agent.avatar} state="asleep" size={22} />
			</View>
			<Text style={styles.stoppedText} numberOfLines={2}>
				{stoppedLabel(stopped, agent.name)}
			</Text>
			{stopped.retry ? (
				<Pressable
					onPress={() => {
						haptic("tap");
						onRetry(stopped);
					}}
					disabled={busy}
					accessibilityRole="button"
					hitSlop={8}
					style={styles.retry}
				>
					{busy ? <ActivityIndicator size="small" color={color.mute} /> : <Text style={styles.retryText}>{t("agents.room.live.retry")}</Text>}
				</Pressable>
			) : null}
		</View>
	);
});

/** Scrolled up while lines came: "3 novas", a tap goes back to the end. */
export function JumpPill({ count, bottom, onPress }: { count: number; bottom: number; onPress: () => void }) {
	const { color } = useTheme();
	const styles = useStyles();
	if (count <= 0) return null;
	return (
		<Pressable
			onPress={() => {
				haptic("tap");
				onPress();
			}}
			accessibilityRole="button"
			accessibilityLabel={`${newLinesLabel(count)}. ${t("agents.room.jumpTip")}`}
			style={[styles.jump, { bottom }]}
		>
			<Feather name="arrow-down" size={14} color={color.accentInk} />
			<Text style={styles.jumpText}>{newLinesLabel(count)}</Text>
		</Pressable>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		row: { flexDirection: "row", alignItems: "flex-start", gap: 10 },
		face: { width: FACE, alignItems: "center" },
		body: { flex: 1, minWidth: 0, gap: 5 },
		head: { flexDirection: "row", alignItems: "center", gap: 8, minHeight: 24 },
		name: { color: color.fg, fontSize: 14, fontWeight: "700", flexShrink: 1 },
		clock: { fontSize: 11, color: color.faint },
		action: { fontSize: 12, color: color.mute, fontWeight: "500" },
		stop: { flexDirection: "row", alignItems: "center", gap: 6, paddingHorizontal: 10, height: 28, borderRadius: 14, backgroundColor: color.chip },
		stopText: { color: color.fg, fontSize: 12, fontWeight: "500" },
		doing: { color: color.accent, fontSize: 13, fontWeight: "500" },
		steps: { gap: 3 },
		more: { color: color.faint, fontSize: 11 },
		stopped: { flexDirection: "row", alignItems: "center", gap: 10 },
		stoppedText: { flex: 1, color: color.mute, fontSize: 12, lineHeight: 17 },
		retry: { minHeight: 32, paddingHorizontal: 12, borderRadius: 16, alignItems: "center", justifyContent: "center", borderWidth: 1, borderColor: color.border },
		retryText: { color: color.fg, fontSize: 12, fontWeight: "500" },
		jump: {
			position: "absolute",
			alignSelf: "center",
			flexDirection: "row",
			alignItems: "center",
			gap: 6,
			paddingHorizontal: 14,
			height: 34,
			borderRadius: 17,
			backgroundColor: color.accent,
			shadowColor: "#000",
			shadowOpacity: 0.18,
			shadowRadius: 8,
			shadowOffset: { width: 0, height: 2 },
			elevation: 4,
			borderWidth: 1,
			borderColor: alpha(color.accentInk, 0.12),
		},
		jumpText: { color: color.accentInk, fontSize: 13, fontWeight: "600" },
	}),
);
