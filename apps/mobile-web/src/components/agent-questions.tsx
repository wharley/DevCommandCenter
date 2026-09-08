import { useState } from "react";
import type { QuestionRequest } from "@/lib/questions";
import type { PairingSession } from "@/lib/session";
import { apiFetch } from "@/lib/api";
import { readLocal, removeLocal, writeLocal } from "@/lib/local-data";
export function AgentQuestions({
	request,
	session,
	threadId,
	onAnswered,
}: {
	request: QuestionRequest;
	session: PairingSession;
	threadId: string;
	onAnswered: () => void;
}) {
	const key = `questions:${threadId}:${request.requestId}`;
	const [answers, setAnswers] = useState<Record<string, string>>(
		() => readLocal(session, key) ?? {},
	);
	const [busy, setBusy] = useState(false);
	const [sent, setSent] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const change = (id: string, value: string) => {
		const next = { ...answers, [id]: value };
		setAnswers(next);
		writeLocal(session, key, next);
	};
	const submit = async () => {
		if (busy || sent) return;
		setBusy(true);
		setError(null);
		try {
			await apiFetch(
				session,
				`/api/v1/sessions/${encodeURIComponent(threadId)}/respond-user-input`,
				{
					method: "POST",
					body: JSON.stringify({
						sessionId: threadId,
						requestId: request.requestId,
						answers: request.questions.map((q) => ({
							question: q.question,
							answer: answers[q.id]?.trim() ?? "",
						})),
					}),
				},
			);
			setSent(true);
			removeLocal(session, key);
			onAnswered();
		} catch (e) {
			setError(e instanceof Error ? e.message : "Falha ao responder.");
		} finally {
			setBusy(false);
		}
	};
	return (
		<section className="my-4 rounded-2xl border border-wait/40 bg-wait/5 p-4">
			<h2 className="mb-3 text-sm font-semibold text-wait">
				O agente precisa de você
			</h2>
			{request.questions.map((q) => (
				<fieldset disabled={busy || sent} key={q.id} className="mb-4 space-y-2">
					<legend className="mb-2 text-sm">{q.question}</legend>
					{q.options.map((option) => (
						<button
							type="button"
							key={option.label}
							aria-pressed={answers[q.id] === option.label}
							onClick={() => change(q.id, option.label)}
							className={`block w-full rounded-xl border p-3 text-left text-sm ${answers[q.id] === option.label ? "border-wait" : "border-border"}`}
						>
							{option.label}
							{option.description && (
								<small className="mt-1 block text-mute">
									{option.description}
								</small>
							)}
						</button>
					))}
					<textarea
						aria-label={`Resposta: ${q.question}`}
						className="w-full rounded-xl border border-border bg-panel p-3 text-base"
						placeholder="Ou escreva sua resposta…"
						value={answers[q.id] ?? ""}
						onChange={(e) => change(q.id, e.target.value)}
					/>
				</fieldset>
			))}
			{error && (
				<p role="alert" className="mb-3 text-xs text-danger">
					{error}
				</p>
			)}
			<button
				type="button"
				disabled={
					busy || sent || request.questions.some((q) => !answers[q.id]?.trim())
				}
				onClick={() => void submit()}
				className="min-h-11 w-full rounded-xl bg-wait p-3 text-sm font-semibold text-wait-ink disabled:opacity-50"
			>
				{sent ? "Resposta enviada" : busy ? "Enviando…" : "Enviar respostas"}
			</button>
		</section>
	);
}
