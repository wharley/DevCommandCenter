/**
 * A room's cards on the phone (`@dcc/mobile-core` roomCards), in the
 * desktop's look made for a thumb: the agent's face, its trigger chip
 * ("Gatilho · CI falhou"), one line ("Dev terminou em Checkout"), the
 * verdict of a pull request review ("Pode aprovar"), the time and a dot
 * while unread; a tap opens where the result is (the task's thread, or a
 * check-in's link). DCC's own cards carry their buttons: "Criar" /
 * "Descartar" on a proposal, "Continuar mesmo assim" on a limit.
 */
import {
	roomCardDetail,
	roomCardText,
	roomClock,
	roomDayLabel,
	roomTriggerLabel,
	roomVerdictLabel,
	t,
	type ResidentAgent,
	type RoomCard,
	type RoomVerdict,
} from "@dcc/mobile-core";
import { alpha, makeStyles, radius, useTheme } from "@/theme";
import { haptic } from "@/ui/haptics";
import { Button, Mono } from "@/ui/primitives";
import { ResidentMascot } from "@/ui/ResidentMascot";
import Feather from "@expo/vector-icons/Feather";
import { memo } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";

const FACE = 24;

export type RoomCardRowProps = {
	card: RoomCard;
	/** None for a check-in, or an agent the phone does not know yet. */
	agent: ResidentAgent | undefined;
	/** One of its buttons is on its way. */
	busy: boolean;
	onOpen: (card: RoomCard, where: "thread" | "delivery") => void;
	onAct: (card: RoomCard, action: "create" | "discard" | "continue") => void;
};

/** "Hoje 09:41", "Ontem 18:02", "3 de nov. 10:00". */
function when(at: string | null): string {
	if (!at) return "";
	return `${roomDayLabel(at)} ${roomClock(at)}`.trim();
}

function VerdictChip({ verdict }: { verdict: RoomVerdict }) {
	const { color } = useTheme();
	const styles = useStyles();
	const tint = verdict === "approve" ? color.accent : verdict === "requestChanges" ? color.danger : color.wait;
	return (
		<View style={[styles.chip, { backgroundColor: alpha(tint, 0.14), borderColor: alpha(tint, 0.35) }]}>
			<Text style={[styles.chipText, { color: tint }]}>{roomVerdictLabel(verdict)}</Text>
		</View>
	);
}

function CardRowView({ card, agent, busy, onOpen, onAct }: RoomCardRowProps) {
	const { color } = useTheme();
	const styles = useStyles();
	const name = agent?.name ?? "O agente";
	const line = roomCardText(card, name);
	const trigger = roomTriggerLabel(card.trigger);
	const own = card.kind === "limit" || card.actions.includes("create");
	const waiting = card.kind === "taskWaiting";
	const amber = waiting || card.kind === "limit";
	const opens = card.actions.includes("open");
	const face =
		card.kind === "checkIn" ? (
			<Feather
				name={card.checkIn?.kind === "taskStalled" ? "clock" : card.checkIn?.kind === "issueAssigned" ? "circle" : "git-pull-request"}
				size={18}
				color={color.mute}
			/>
		) : card.kind === "agentProposal" && card.proposal ? (
			<ResidentMascot avatar={card.proposal.avatar} state="watching" size={FACE} />
		) : agent ? (
			<ResidentMascot avatar={agent.avatar} state={waiting ? "needsYou" : "watching"} size={FACE} />
		) : null;
	const body = (
		<>
			<View style={styles.face}>{face}</View>
			<View style={styles.body}>
				{trigger || card.kind === "checkIn" ? (
					<View style={styles.chips}>
						<View style={styles.tag}>
							<Text style={styles.tagText}>{trigger ?? t("agents.room.checkIn.tag")}</Text>
						</View>
					</View>
				) : null}
				<Text style={[styles.text, card.unread && styles.unread]} numberOfLines={own ? 3 : 1}>
					{line}
				</Text>
				{card.verdict ? (
					<View style={styles.chips}>
						<VerdictChip verdict={card.verdict} />
					</View>
				) : null}
				{roomCardDetail(card).map((detail, index) => (
					<Text key={index} style={styles.detail} numberOfLines={index === 0 ? 3 : 2}>
						{detail}
					</Text>
				))}
				{own ? (
					<View style={styles.actions}>
						{card.kind === "limit" ? (
							<Button
								label={t("agents.spend.continue")}
								variant="wait"
								style={styles.action}
								busy={busy}
								disabled={busy}
								onPress={() => {
									haptic("success");
									onAct(card, "continue");
								}}
							/>
						) : (
							<>
								<Button
									label={t("agents.proposal.create")}
									style={styles.action}
									busy={busy}
									disabled={busy}
									onPress={() => {
										haptic("success");
										onAct(card, "create");
									}}
								/>
								<Button
									label={t("agents.proposal.discard")}
									variant="ghost"
									style={styles.action}
									disabled={busy}
									onPress={() => {
										haptic("tap");
										onAct(card, "discard");
									}}
								/>
							</>
						)}
					</View>
				) : null}
				{card.actions.includes("delivery") && card.workspaceId ? (
					<Pressable
						onPress={() => {
							haptic("tap");
							onOpen(card, "delivery");
						}}
						accessibilityRole="button"
						accessibilityLabel={`Ver as mudanças de ${card.title}`}
						hitSlop={8}
						style={styles.link}
					>
						<Text style={styles.linkText}>Ver mudanças</Text>
					</Pressable>
				) : null}
			</View>
			<View style={styles.side}>
				{when(card.at) && !own ? <Mono style={styles.when}>{when(card.at)}</Mono> : null}
				{card.unread ? <View style={[styles.dot, { backgroundColor: waiting ? color.waitFill : color.info }]} /> : null}
			</View>
		</>
	);
	const frame = [styles.card, amber && { borderColor: alpha(color.waitFill, 0.4), backgroundColor: alpha(color.waitFill, 0.06) }];
	if (!opens) {
		return (
			<View style={frame} accessible={!own} accessibilityLabel={line}>
				{body}
			</View>
		);
	}
	return (
		<Pressable
			onPress={() => {
				haptic("tap");
				onOpen(card, "thread");
			}}
			accessibilityRole="button"
			accessibilityLabel={[trigger, line, card.verdict ? roomVerdictLabel(card.verdict) : null, card.unread ? "não lido" : null]
				.filter(Boolean)
				.join(". ")}
			style={({ pressed }) => [...frame, pressed && { backgroundColor: color.elevated }]}
		>
			{body}
		</Pressable>
	);
}

/** Redrawn only when its card, its agent's face or name, or its state moved. */
export const RoomCardRow = memo(
	CardRowView,
	(before, after) =>
		before.card === after.card &&
		before.busy === after.busy &&
		before.agent?.name === after.agent?.name &&
		JSON.stringify(before.agent?.avatar) === JSON.stringify(after.agent?.avatar) &&
		before.onOpen === after.onOpen &&
		before.onAct === after.onAct,
);

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		card: {
			flexDirection: "row",
			alignItems: "flex-start",
			gap: 10,
			paddingHorizontal: 12,
			paddingVertical: 10,
			borderRadius: radius.md,
			borderWidth: 1,
			borderColor: alpha(color.border, 0.9),
			backgroundColor: color.panel,
		},
		face: { width: FACE + 6, minHeight: FACE, alignItems: "center", justifyContent: "center" },
		body: { flex: 1, minWidth: 0, gap: 4 },
		chips: { flexDirection: "row", flexWrap: "wrap", gap: 6 },
		tag: { paddingHorizontal: 6, paddingVertical: 1, borderRadius: 4, backgroundColor: color.chip, alignSelf: "flex-start" },
		tagText: { color: color.mute, fontSize: 10.5 },
		chip: { paddingHorizontal: 8, paddingVertical: 2, borderRadius: 999, borderWidth: 1, alignSelf: "flex-start" },
		chipText: { fontSize: 11.5, fontWeight: "600" },
		text: { color: color.fg, fontSize: 14, lineHeight: 19 },
		unread: { fontWeight: "600" },
		detail: { color: color.mute, fontSize: 12.5, lineHeight: 17 },
		actions: { flexDirection: "row", gap: 8, marginTop: 4 },
		action: { flex: 1, minHeight: 38 },
		link: { alignSelf: "flex-start", paddingTop: 2 },
		linkText: { color: color.mute, fontSize: 12, fontWeight: "500" },
		side: { alignItems: "flex-end", gap: 6, paddingTop: 2 },
		when: { fontSize: 10.5, color: color.faint },
		dot: { width: 7, height: 7, borderRadius: 4 },
	}),
);
