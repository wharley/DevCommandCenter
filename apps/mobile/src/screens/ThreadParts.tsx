/** Pieces of the thread view: bubbles, the trace capsule, permission and question cards. */
import {
	stepIsLive,
	stepLabel,
	stepParts,
	t,
	traceTitle,
	type ChatMessage,
	type QuestionRequest,
	type TraceStep,
} from "@dcc/mobile-core";
import Feather from "@expo/vector-icons/Feather";
import { useState } from "react";
import { ActivityIndicator, Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import { alpha, color, font, radius } from "../theme";
import { RichText } from "../ui/RichText";
import { Button, Mono, StateDot } from "../ui/primitives";

export function UserBubble({ text }: { text: string }) {
	return (
		<View style={styles.userWrap}>
			<View style={styles.userBubble}>
				<Text style={styles.userText} selectable>
					{text || "(vazio)"}
				</Text>
			</View>
		</View>
	);
}

export function AssistantBlock({ message }: { message: Extract<ChatMessage, { kind: "assistant" }> }) {
	return (
		<View style={{ gap: 6 }}>
			{message.text ? <RichText text={message.text} /> : !message.completed ? <ActivityIndicator size="small" color={color.mute} style={{ alignSelf: "flex-start" }} /> : null}
			{!message.completed && message.text ? <StateDot state="live" size={7} /> : null}
			{message.aborted ? <Mono style={styles.aborted}>interrompido</Mono> : null}
		</View>
	);
}

export function SystemRow({ text }: { text: string }) {
	return (
		<View style={styles.systemRow}>
			<View style={styles.rule} />
			<Mono style={styles.systemText}>{text}</Mono>
			<View style={styles.rule} />
		</View>
	);
}

export function TraceBlock({ steps }: { steps: TraceStep[] }) {
	const [open, setOpen] = useState(false);
	const live = steps.some(stepIsLive);
	const current = live ? [...steps].reverse().find(stepIsLive) : null;
	const failed = steps.filter((s) => s.kind === "tool" && s.status === "failed").length;
	return (
		<View style={styles.trace}>
			<Pressable onPress={() => setOpen((v) => !v)} style={styles.traceHead} accessibilityRole="button" accessibilityState={{ expanded: open }}>
				<Feather name={open ? "chevron-down" : "chevron-right"} size={13} color={color.faint} />
				<Mono style={styles.traceLabel}>{traceTitle(steps)}</Mono>
				<View style={styles.traceCount} accessibilityLabel={t("conversation.activity.turn.steps", { count: steps.length })}>
					<Mono style={styles.traceCountText}>{steps.length}</Mono>
				</View>
				{current ? (
					<>
						<ActivityIndicator size="small" color={color.accent} style={{ transform: [{ scale: 0.7 }] }} />
						<Mono style={styles.traceCurrent} numberOfLines={1}>
							{stepLabel(current)}
						</Mono>
					</>
				) : (
					<View style={{ flex: 1 }} />
				)}
				{failed > 0 ? (
					<View style={styles.traceFailedWrap} accessibilityLabel={`${t("conversation.activity.timeline.filters.failures")}: ${failed}`}>
						<Feather name="x" size={11} color={color.danger} />
						<Mono style={styles.traceFailed}>{failed}</Mono>
					</View>
				) : null}
			</Pressable>
			{open ? (
				<View style={styles.traceBody}>
					{steps.map((step, i) => (
						<StepRow key={i} step={step} />
					))}
				</View>
			) : null}
		</View>
	);
}

function StepRow({ step }: { step: TraceStep }) {
	const [open, setOpen] = useState(false);
	const live = stepIsLive(step);
	const failed = step.kind === "tool" && step.status === "failed";
	const { verb, target } = stepParts(step);
	// A permission's first line is already its label; the rest is the detail.
	const detail =
		step.kind === "tool"
			? detailText(step.input)
			: step.kind === "permission"
				? step.question.split("\n").slice(1).join("\n").trim()
				: "";
	return (
		<View>
			<Pressable disabled={!detail && !(step.kind === "tool" && step.error)} onPress={() => setOpen((v) => !v)} style={styles.stepRow}>
				{live ? (
					<ActivityIndicator size="small" color={color.accent} style={{ transform: [{ scale: 0.6 }], width: 14 }} />
				) : (
					<Feather name={failed ? "x" : "check"} size={12} color={failed ? color.danger : color.faint} style={{ width: 14 }} />
				)}
				<Text style={[styles.stepLabel, failed && { color: color.danger }]} numberOfLines={1}>
					{verb}
				</Text>
				{target || (detail && !open) ? (
					<Text style={styles.stepPreview} numberOfLines={1}>
						{target ?? detail.split("\n", 1)[0]}
					</Text>
				) : (
					<View style={{ flex: 1 }} />
				)}
			</Pressable>
			{open ? (
				<View style={styles.stepDetail}>
					{detail ? (
						<Text style={styles.snippet} selectable>
							{detail}
						</Text>
					) : null}
					{step.kind === "tool" && step.error ? <Text style={[styles.snippet, { color: color.danger }]}>{step.error}</Text> : null}
				</View>
			) : null}
		</View>
	);
}

function detailText(input: unknown): string {
	if (input === null || input === undefined) return "";
	if (typeof input === "string") return input;
	if (typeof input === "object") {
		const record = input as Record<string, unknown>;
		const command = typeof record.command === "string" ? record.command : null;
		const file = typeof record.file === "string" ? record.file : null;
		if (command || file) return [command, file].filter(Boolean).join("\n");
		if (Object.values(record).every((value) => value === undefined || value === null)) return "";
		try {
			return JSON.stringify(input, null, 2);
		} catch {
			return "";
		}
	}
	return String(input);
}

export function PermissionCard({
	message,
	busy,
	onRespond,
}: {
	message: Extract<ChatMessage, { kind: "permission" }>;
	busy: boolean;
	onRespond: (choice: string) => void;
}) {
	return (
		<View style={styles.waitCard}>
			<View style={styles.waitHead}>
				<StateDot state="wait" />
				<Mono style={styles.waitEyebrow}>pedido de permissão</Mono>
			</View>
			<Mono style={styles.permissionText} selectable>
				{message.question}
			</Mono>
			<View style={{ flexDirection: "row", gap: 8 }}>
				{message.choices.map((choice) => (
					<Button
						key={choice.id}
						label={choice.label}
						variant={choice.id === "deny" ? "ghost" : "wait"}
						busy={busy && choice.id !== "deny"}
						disabled={busy}
						style={{ flex: 1 }}
						onPress={() => onRespond(choice.id)}
					/>
				))}
			</View>
		</View>
	);
}

export function QuestionCard({
	request,
	busy,
	onSubmit,
}: {
	request: QuestionRequest;
	busy: boolean;
	onSubmit: (answers: Array<{ question: string; answer: string }>) => void;
}) {
	const [answers, setAnswers] = useState<Record<string, string>>({});
	const complete = request.questions.every((q) => (answers[q.id] ?? "").trim());
	return (
		<View style={styles.waitCard}>
			<View style={styles.waitHead}>
				<StateDot state="wait" />
				<Mono style={styles.waitEyebrow}>o agente perguntou</Mono>
			</View>
			{request.questions.map((q) => (
				<View key={q.id} style={{ gap: 8 }}>
					{q.header ? <Mono style={styles.questionHeader}>{q.header}</Mono> : null}
					<Text style={styles.questionText}>{q.question}</Text>
					{q.options.map((option) => {
						const selected = answers[q.id] === option.label;
						return (
							<Pressable
								key={option.label}
								onPress={() => setAnswers((prev) => ({ ...prev, [q.id]: option.label }))}
								style={[styles.option, selected && styles.optionSelected]}
								accessibilityRole="radio"
								accessibilityState={{ selected }}
							>
								<Text style={styles.optionLabel}>{option.label}</Text>
								{option.description ? <Text style={styles.optionDesc}>{option.description}</Text> : null}
							</Pressable>
						);
					})}
					<TextInput
						value={q.options.some((o) => o.label === answers[q.id]) ? "" : (answers[q.id] ?? "")}
						onChangeText={(text) => setAnswers((prev) => ({ ...prev, [q.id]: text }))}
						placeholder={q.options.length ? "Ou escreva outra resposta" : "Sua resposta"}
						placeholderTextColor={color.faint}
						style={styles.answerInput}
						multiline
					/>
				</View>
			))}
			<Button
				label="Responder"
				variant="wait"
				busy={busy}
				disabled={!complete}
				onPress={() =>
					onSubmit(request.questions.map((q) => ({ question: q.question, answer: (answers[q.id] ?? "").trim() })))
				}
			/>
		</View>
	);
}

const styles = StyleSheet.create({
	userWrap: { alignItems: "flex-end" },
	userBubble: {
		maxWidth: "85%",
		backgroundColor: color.accent,
		borderRadius: radius.lg,
		borderBottomRightRadius: 4,
		paddingHorizontal: 14,
		paddingVertical: 8,
	},
	userText: { color: color.accentInk, fontSize: 15, lineHeight: 21 },
	aborted: { fontSize: 10, letterSpacing: 1.8, textTransform: "uppercase", color: alpha(color.danger, 0.8) },
	systemRow: { flexDirection: "row", alignItems: "center", gap: 8, paddingVertical: 2 },
	rule: { flex: 1, height: StyleSheet.hairlineWidth, backgroundColor: color.border },
	systemText: { fontSize: 10, color: color.faint, letterSpacing: 1.2, textTransform: "uppercase" },
	trace: { borderWidth: 1, borderColor: alpha(color.border, 0.9), backgroundColor: alpha(color.panel, 0.6), borderRadius: radius.md, overflow: "hidden" },
	traceHead: { flexDirection: "row", alignItems: "center", gap: 8, paddingHorizontal: 12, paddingVertical: 9 },
	traceLabel: { fontSize: 10, fontWeight: "600", letterSpacing: 1.8, textTransform: "uppercase", color: color.mute },
	traceCount: { minWidth: 17, height: 17, borderRadius: 9, backgroundColor: color.elevated, alignItems: "center", justifyContent: "center", paddingHorizontal: 4 },
	traceCountText: { fontSize: 10, fontWeight: "700", color: color.mute },
	traceCurrent: { flex: 1, fontSize: 11, color: color.accent },
	traceFailedWrap: { flexDirection: "row", alignItems: "center", gap: 2 },
	traceFailed: { fontSize: 10, color: color.danger },
	traceBody: { borderTopWidth: 1, borderTopColor: color.border, paddingHorizontal: 6, paddingVertical: 6 },
	stepRow: { flexDirection: "row", alignItems: "center", gap: 8, paddingHorizontal: 6, paddingVertical: 6 },
	stepLabel: { fontSize: 13, color: color.fg, flexShrink: 0, maxWidth: "50%" },
	stepPreview: { flex: 1, fontSize: 12, color: color.faint, fontFamily: font.mono },
	stepDetail: { marginLeft: 28, marginBottom: 6, marginRight: 6, gap: 6 },
	snippet: { fontFamily: font.mono, fontSize: 11, color: color.fg, backgroundColor: color.bg, borderRadius: 6, padding: 8 },
	waitCard: {
		borderWidth: 1,
		borderColor: alpha(color.wait, 0.4),
		backgroundColor: alpha(color.wait, 0.05),
		borderRadius: radius.lg,
		padding: 14,
		gap: 12,
	},
	waitHead: { flexDirection: "row", alignItems: "center", gap: 8 },
	waitEyebrow: { fontSize: 10, fontWeight: "600", letterSpacing: 1.8, textTransform: "uppercase", color: color.wait },
	permissionText: { fontSize: 12, lineHeight: 18 },
	questionHeader: { fontSize: 10, letterSpacing: 1.5, textTransform: "uppercase", color: color.mute },
	questionText: { color: color.fg, fontSize: 15, lineHeight: 21 },
	option: { borderWidth: 1, borderColor: color.border, backgroundColor: color.bg, borderRadius: radius.md, padding: 10, gap: 2 },
	optionSelected: { borderColor: color.wait, backgroundColor: alpha(color.wait, 0.1) },
	optionLabel: { color: color.fg, fontSize: 14, fontWeight: "600" },
	optionDesc: { color: color.mute, fontSize: 12 },
	answerInput: {
		borderWidth: 1,
		borderColor: color.border,
		backgroundColor: color.bg,
		borderRadius: radius.md,
		color: color.fg,
		padding: 10,
		fontSize: 14,
		minHeight: 40,
	},
});
