import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { CheckCircle2, LoaderCircle } from "lucide-react";
import type {
	ProviderUserInputAnswer,
	ProviderUserInputQuestion,
} from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { respondToUserInput } from "@/lib/session-api";
import { toast } from "sonner";

type UserInputCardProps = {
	sessionId: string | null;
	requestId: string;
	questions: ProviderUserInputQuestion[];
	answers: ProviderUserInputAnswer[];
	isLive: boolean;
};

function answersToDraft(answers: ProviderUserInputAnswer[]) {
	return Object.fromEntries(
		answers.map((answer) => [answer.question, answer.answer]),
	) as Record<string, string>;
}

export function UserInputCard({
	sessionId,
	requestId,
	questions,
	answers,
	isLive,
}: UserInputCardProps) {
	const { t } = useTranslation("common");
	const [draft, setDraft] = useState<Record<string, string>>(() => answersToDraft(answers));
	const [submitting, setSubmitting] = useState(false);
	const normalizedQuestions = questions.filter((question) => question.question.trim().length > 0);
	const resolved = !isLive && answers.length > 0;
	const canSubmit = useMemo(
		() =>
			Boolean(sessionId) &&
			normalizedQuestions.every((question) => (draft[question.question] ?? "").trim().length > 0),
		[draft, normalizedQuestions, sessionId],
	);

	if (normalizedQuestions.length === 0) {
		return null;
	}

	return (
		<div className="rounded-2xl border border-border/70 bg-card/70 p-4">
			<div className="mb-3 flex items-center justify-between gap-3">
				<div>
					<p className="text-[11px] font-medium uppercase tracking-[0.22em] text-muted-foreground/80">
						{t("conversation.userInput.label")}
					</p>
					<p className="mt-1 text-sm text-foreground">
						{resolved
							? t("conversation.userInput.delivered")
							: t("conversation.userInput.waiting")}
					</p>
				</div>
				{resolved ? (
					<span className="inline-flex items-center gap-1 text-xs text-emerald-600">
						<CheckCircle2 className="size-4" aria-hidden />
						{t("conversation.userInput.resolved")}
					</span>
				) : null}
			</div>

			<div className="space-y-4">
				{normalizedQuestions.map((question) => {
					const selected = draft[question.question] ?? "";
					const options = question.options ?? [];
					return (
						<div key={question.id} className="space-y-2">
							<div>
								<p className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground/70">
									{question.header || t("conversation.userInput.question")}
								</p>
								<p className="mt-1 text-sm text-foreground">{question.question}</p>
							</div>

							{options.length > 0 ? (
								<div className="flex flex-wrap gap-2">
									{options.map((option) => {
										const active = selected === option.label;
										return (
											<Button
												key={`${question.id}-${option.label}`}
												type="button"
												variant={active ? "default" : "outline"}
												size="sm"
												className={cn(
													"h-auto max-w-full whitespace-normal px-3 py-2 text-left",
													!active && "bg-background/60",
												)}
												disabled={!isLive || submitting}
												onClick={() =>
													setDraft((current) => ({
														...current,
														[question.question]: option.label,
													}))
												}
											>
												<span className="text-sm">{option.label}</span>
											</Button>
										);
									})}
								</div>
							) : null}

							<Input
								value={selected}
								disabled={!isLive || submitting}
								placeholder={t("conversation.userInput.placeholder")}
								onChange={(event) =>
									setDraft((current) => ({
										...current,
										[question.question]: event.target.value,
									}))
								}
							/>
						</div>
					);
				})}
			</div>

			{isLive ? (
				<div className="mt-4 flex items-center justify-end gap-2">
					<Button
						type="button"
						size="sm"
						disabled={!canSubmit || submitting}
						onClick={async () => {
							if (!sessionId) {
								return;
							}
							setSubmitting(true);
							try {
								await respondToUserInput({
									sessionId,
									requestId,
									answers: normalizedQuestions.map((question) => ({
										question: question.question,
										answer: (draft[question.question] ?? "").trim(),
									})),
								});
							} catch (error) {
								toast.error(
									error instanceof Error
										? error.message
										: t("conversation.userInput.submitError"),
								);
							} finally {
								setSubmitting(false);
							}
						}}
					>
						{submitting ? <LoaderCircle className="size-4 animate-spin" aria-hidden /> : null}
					{submitting
						? t("conversation.userInput.sending")
						: t("conversation.userInput.send")}
					</Button>
				</div>
			) : null}
		</div>
	);
}
