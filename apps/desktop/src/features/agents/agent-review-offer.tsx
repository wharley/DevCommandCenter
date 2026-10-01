import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { AgentAvatar } from "./agent-avatar";
import { reviewOfferKey } from "./review-offer";
import { dispatchCallAgent } from "./call-agent-command";
import { type AgentView, useAgents } from "./use-agents";
import "./agents.css";

/** Offers the person already answered; kept while the app runs so switching tasks does not ask again. */
const answeredOffers = new Set<string>();

/**
 * Agents floating above the composer after a turn leaves changes, each asking
 * its own question. Nothing runs until the person accepts.
 */
export function AgentReviewOffer({
	workspaceId,
	sessionId,
	sessions,
	changedFileCount,
	scope = "local",
}: {
	workspaceId: string;
	sessionId: string | null;
	sessions: WorkspaceSessionSummary[];
	changedFileCount: number;
	scope?: string;
}) {
	const { t } = useTranslation("common");
	const { agents, agentBySessionId } = useAgents(scope);
	const [, setAnsweredCount] = useState(0);
	const answer = (key: string) => {
		answeredOffers.add(key);
		setAnsweredCount(answeredOffers.size);
	};
	const isAgentSession = sessionId ? agentBySessionId.has(sessionId) : false;
	const offers = agents.flatMap((agent) => {
		const key = reviewOfferKey({
			agent,
			workspaceId,
			sessionId,
			sessions,
			changedFileCount,
			isAgentSession,
		});
		return key && !answeredOffers.has(key) ? [{ agent, key }] : [];
	});
	if (offers.length === 0) {
		return null;
	}

	return (
		<div className="mb-2 flex flex-col gap-1.5">
			{offers.map(({ agent, key }) => (
				<OfferThought
					key={key}
					agent={agent}
					acceptLabel={t("agents.offer.accept")}
					dismissLabel={t("agents.offer.dismiss")}
					onAccept={() => {
						// One agent at a time: accepting answers every pending offer.
						for (const offer of offers) {
							answer(offer.key);
						}
						dispatchCallAgent(agent.id);
					}}
					onDismiss={() => answer(key)}
				/>
			))}
		</div>
	);
}

const THINKING_MS = 900;
const TYPING_MS_PER_CHAR = 28;

function prefersReducedMotion() {
	return (
		typeof window !== "undefined" &&
		window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true
	);
}

/**
 * The mascot thinking out loud: dots first, then the question written out,
 * then the answer buttons. With reduced motion everything shows at once.
 */
export function OfferThought({
	agent,
	question: customQuestion,
	acceptLabel,
	dismissLabel,
	onAccept,
	onDismiss,
}: {
	agent: AgentView;
	/** Asked instead of the agent's own offer text. */
	question?: string;
	acceptLabel: string;
	dismissLabel: string;
	onAccept: () => void;
	onDismiss: () => void;
}) {
	const question = customQuestion ?? agent.offerPrompt;
	const [typed, setTyped] = useState(() => (prefersReducedMotion() ? question.length : -1));
	useEffect(() => {
		if (typed >= question.length) {
			return;
		}
		const timer = window.setTimeout(
			() => setTyped((current) => current + 1),
			typed < 0 ? THINKING_MS : TYPING_MS_PER_CHAR,
		);
		return () => window.clearTimeout(timer);
	}, [question.length, typed]);
	const thinking = typed < 0;
	const ready = typed >= question.length;

	return (
		<div className="dcc-agent-offer flex flex-col items-end">
			<div className="flex min-h-10 items-center gap-2 rounded-[20px] border border-border/70 bg-card py-1.5 pl-4 pr-1.5 shadow-sm">
				{/* Read once, in full, instead of letter by letter. */}
				<p className="sr-only" aria-live="polite">
					{agent.name}: {question}
				</p>
				{thinking ? (
					<span className="dcc-agent-thinking flex items-center gap-1 py-2 pr-2.5" aria-hidden>
						<span />
						<span />
						<span />
					</span>
				) : (
					<p className="py-1 text-[13px] text-foreground" aria-hidden>
						<span className="font-medium">{agent.name}:</span> {question.slice(0, typed)}
						{!ready && <span className="dcc-agent-caret" />}
					</p>
				)}
				{ready && (
					<span className="dcc-agent-offer-actions flex items-center gap-1 pl-1">
						<Button type="button" size="sm" onClick={onAccept}>
							<span>{acceptLabel}</span>
						</Button>
						<Button
							type="button"
							variant="ghost"
							size="icon-sm"
							aria-label={dismissLabel}
							onClick={onDismiss}
						>
							<X className="size-3.5" />
						</Button>
					</span>
				)}
			</div>
			{/* The thought rises from the mascot's head: small bubble near the
			    head, larger one near the balloon, both centred over the mascot. */}
			<div className="flex w-12 flex-col items-center gap-1 py-1" aria-hidden>
				<span className="size-2.5 rounded-full border border-border/70 bg-card" />
				<span className="size-1.5 rounded-full border border-border/70 bg-card" />
			</div>
			<AgentAvatar avatar={agent.avatar} size={48} className="dcc-agent-float shrink-0" />
		</div>
	);
}
