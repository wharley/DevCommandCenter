/**
 * A task an agent proposed, started or dismissed from the phone. The desktop
 * shows the same proposal in the thread; whoever answers first wins.
 */
import { proposalMeta, TASK_PROPOSAL_HINT, type TaskProposal } from "@dcc/mobile-core";
import { useState, type ReactNode } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import type { TaskProposalAnswer } from "../backend/taskProposals";
import { alpha, makeStyles, radius } from "../theme";
import { Button, Mono, StateDot } from "./primitives";

/** Briefs longer than this open collapsed. */
const COLLAPSED_LINES = 6;

export function TaskProposalCard({
	proposal,
	busy,
	onAnswer,
	header,
}: {
	proposal: TaskProposal;
	busy: boolean;
	onAnswer: (action: TaskProposalAnswer) => void;
	/** The thread that proposed it, on the home screen. */
	header?: ReactNode;
}) {
	const styles = useStyles();
	const [open, setOpen] = useState(false);
	return (
		<View style={styles.card} accessibilityLabel={`Tarefa proposta: ${proposal.title}`}>
			{header}
			<View style={styles.head}>
				<StateDot state="wait" />
				<Mono style={styles.eyebrow}>tarefa proposta</Mono>
			</View>
			<Text style={styles.title}>{proposal.title}</Text>
			<Mono style={styles.meta}>{proposalMeta(proposal)}</Mono>
			{proposal.brief ? (
				<Pressable onPress={() => setOpen((value) => !value)} accessibilityRole="button" accessibilityState={{ expanded: open }}>
					<Text style={styles.brief} selectable numberOfLines={open ? undefined : COLLAPSED_LINES}>
						{proposal.brief}
					</Text>
				</Pressable>
			) : null}
			<Text style={styles.hint}>{TASK_PROPOSAL_HINT}</Text>
			<View style={styles.choices}>
				<Button label="Descartar" variant="ghost" disabled={busy} style={{ flex: 1 }} onPress={() => onAnswer("dismiss")} />
				<Button label="Começar" variant="wait" busy={busy} disabled={busy} style={{ flex: 1 }} onPress={() => onAnswer("start")} />
			</View>
		</View>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		card: {
			borderWidth: 1,
			borderColor: alpha(color.waitFill, 0.4),
			backgroundColor: alpha(color.waitFill, 0.05),
			borderRadius: radius.lg,
			padding: 14,
			gap: 8,
		},
		head: { flexDirection: "row", alignItems: "center", gap: 8 },
		eyebrow: { fontSize: 10, letterSpacing: 0.8, color: color.wait, textTransform: "uppercase", flex: 1 },
		title: { color: color.fg, fontSize: 15, fontWeight: "600" },
		meta: { fontSize: 11, color: color.mute },
		brief: { color: color.fg, fontSize: 13, lineHeight: 19 },
		hint: { color: color.mute, fontSize: 12, lineHeight: 17 },
		choices: { flexDirection: "row", gap: 8, marginTop: 4 },
	}),
);
