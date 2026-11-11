/**
 * An agent's question docked on the room's field (`app/room.tsx`): the
 * thread's own question card, headed by who asks ("Revisor pergunta"),
 * answered with the session's `respond-user-input`. One at a time, the
 * oldest first, then "+N perguntas esperando"; tall questions scroll inside
 * the dock so the field stays in reach.
 */
import { questionTitle, type RoomQuestion } from "@dcc/mobile-core";
import { makeStyles } from "@/theme";
import { ScrollView, StyleSheet, Text } from "react-native";
import { QuestionCard } from "./ThreadParts";

export function QuestionDock({
	questions,
	answering,
	onAnswer,
}: {
	questions: RoomQuestion[];
	answering: ReadonlySet<string>;
	onAnswer: (question: RoomQuestion, answers: Array<{ question: string; answer: string }>) => void;
}) {
	const styles = useStyles();
	const question = questions[0];
	if (!question) return null;
	const more = questions.length - 1;
	return (
		<ScrollView style={styles.dock} keyboardShouldPersistTaps="handled">
			<QuestionCard
				// A new request starts clean; the same one keeps its picks.
				key={question.requestId}
				request={{ requestId: question.requestId, questions: question.questions, at: "" }}
				busy={answering.has(question.requestId)}
				title={questionTitle(question)}
				onSubmit={(answers) => onAnswer(question, answers)}
			/>
			{more > 0 ? (
				<Text style={styles.more}>
					+{more} {more === 1 ? "pergunta esperando" : "perguntas esperando"}
				</Text>
			) : null}
		</ScrollView>
	);
}

const useStyles = makeStyles((color) =>
	StyleSheet.create({
		dock: { maxHeight: 340, flexGrow: 0 },
		more: { color: color.wait, fontSize: 12, paddingHorizontal: 14, paddingTop: 6 },
	}),
);
