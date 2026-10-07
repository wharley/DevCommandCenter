import { Loader2, X } from "lucide-react";
import { type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { formatDiskBytes } from "@/features/workspaces/workspace-disk-usage";
import { AgentAvatar } from "./agent-avatar";
import { useThoughtTyping } from "./agent-review-offer";
import {
	hasReporterWalked,
	markReporterWalked,
	type ReporterCleanupOffer,
	type ReporterRecapNews,
	reporterSpeech,
	reporterSteppedOut,
	useReporterNews,
} from "./reporter-store";
import { type AgentView, useAgents } from "./use-agents";
import "./agents.css";

/** Where the reporter's mascot sits in the sidebar; the walk starts there. */
export const REPORTER_HOME_ATTRIBUTE = "data-reporter-home";
const SIDEBAR_AVATAR_SIZE = 22;
const AVATAR_SIZE = 48;

function prefersReducedMotion() {
	return (
		typeof window !== "undefined" &&
		window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true
	);
}

type Speech = NonNullable<ReturnType<typeof reporterSpeech>>;

/**
 * The reporter above the composer when it has news: a new recap, or
 * completed tasks past the person's limit. It walks over from its sidebar row,
 * stops in the middle, and thinks out loud, like the reviewer's offer. One
 * thing at a time; the sidebar stays for what needs the person.
 */
export function ReporterOffer() {
	const speech = reporterSpeech(useReporterNews());
	const { agents } = useAgents();
	const reporter = agents.find((agent) => agent.preset === "chronicler");
	if (!speech || !reporter) {
		return null;
	}
	return <ReporterStage agent={reporter} speech={speech} />;
}

type Flight = {
	/** Where the mascot lands, in viewport pixels. */
	left: number;
	top: number;
	/** Offset from the landing spot to its sidebar row. */
	fromX: number;
	fromY: number;
};

function ReporterStage({ agent, speech }: { agent: AgentView; speech: Speech }) {
	const placeholderRef = useRef<HTMLDivElement>(null);
	const flyerRef = useRef<HTMLDivElement>(null);
	const [arrived, setArrived] = useState(() => hasReporterWalked() || prefersReducedMotion());
	const [flight, setFlight] = useState<Flight | null>(null);

	useEffect(() => reporterSteppedOut(), []);

	// The landing spot is the mascot's place above the composer. The walk is
	// drawn in a fixed layer over the whole window, so it crosses from the
	// sidebar instead of being clipped at the panel's edge. Without the
	// sidebar row on screen (collapsed sidebar) it walks in from the left.
	useLayoutEffect(() => {
		const placeholder = placeholderRef.current;
		if (arrived || !placeholder) {
			return;
		}
		const end = placeholder.getBoundingClientRect();
		// The first row on screen: a hidden rail measures as an empty box.
		const home = Array.from(document.querySelectorAll(`[${REPORTER_HOME_ATTRIBUTE}]`))
			.map((element) => element.getBoundingClientRect())
			.find((rect) => rect.width > 0 && rect.height > 0);
		const endX = end.left + end.width / 2;
		const endY = end.top + end.height / 2;
		setFlight({
			left: end.left,
			top: end.top,
			fromX: home ? home.left + home.width / 2 - endX : -Math.min(endX, 360),
			fromY: home ? home.top + home.height / 2 - endY : 0,
		});
	}, [arrived]);

	useLayoutEffect(() => {
		const flyer = flyerRef.current;
		if (!flight || !flyer) {
			return;
		}
		const { fromX, fromY } = flight;
		const startScale = SIDEBAR_AVATAR_SIZE / AVATAR_SIZE;
		const duration = Math.min(2200, Math.max(1000, Math.hypot(fromX, fromY) * 1.6));
		// A small hop on the way, growing from the sidebar's size to its own.
		const animation = flyer.animate(
			[
				{ transform: `translate(${fromX}px, ${fromY}px) scale(${startScale})` },
				{
					transform: `translate(${fromX * 0.4}px, ${fromY * 0.4 - 28}px) scale(0.8)`,
					offset: 0.55,
				},
				{ transform: "translate(0, 0) scale(1)" },
			],
			{ duration, easing: "cubic-bezier(0.33, 0.7, 0.35, 1)", fill: "forwards" },
		);
		animation.onfinish = () => {
			markReporterWalked();
			setFlight(null);
			setArrived(true);
		};
		return () => animation.cancel();
	}, [flight]);

	return (
		<div className="mb-2 flex flex-col items-center">
			{/* A new subject restarts the thought: dots, then its own text. */}
			{arrived &&
				(speech.kind === "recap" ? (
					<RecapThought key="recap" agent={agent} recap={speech.recap} />
				) : (
					<CleanupThought key="cleanup" agent={agent} offer={speech.cleanup} />
				))}
			{arrived && (
				// The thought rises from the mascot's head, like the reviewer's.
				<div className="flex w-12 flex-col items-center gap-1 py-1" aria-hidden>
					<span className="size-2.5 rounded-full border border-border/70 bg-card" />
					<span className="size-1.5 rounded-full border border-border/70 bg-card" />
				</div>
			)}
			<div ref={placeholderRef} className={arrived ? undefined : "invisible"}>
				<AgentAvatar avatar={agent.avatar} size={AVATAR_SIZE} className="dcc-agent-float" />
			</div>
			{flight &&
				createPortal(
					<div
						ref={flyerRef}
						aria-hidden
						className="pointer-events-none fixed z-50 will-change-transform"
						style={{ left: flight.left, top: flight.top }}
					>
						<AgentAvatar avatar={agent.avatar} size={AVATAR_SIZE} className="dcc-agent-working" />
					</div>,
					document.body,
				)}
		</div>
	);
}

function RecapThought({ agent, recap }: { agent: AgentView; recap: ReporterRecapNews }) {
	const { t } = useTranslation("common");
	return (
		<Thought agent={agent} text={recap.text} onDismiss={recap.onDismiss}>
			<Button type="button" size="sm" onClick={recap.onOpen}>
				<span>{t("agents.recap.bubble.open")}</span>
			</Button>
		</Thought>
	);
}

function CleanupThought({ agent, offer }: { agent: AgentView; offer: ReporterCleanupOffer }) {
	const { t, i18n } = useTranslation("common");
	// While tasks are being deleted the totals shrink under the bubble; it
	// keeps saying what it said when the person clicked.
	const settled = useRef(offer.alert);
	if (!offer.alert.progress) settled.current = offer.alert;
	const alert = offer.alert.progress
		? { ...settled.current, progress: offer.alert.progress }
		: offer.alert;
	const size = (bytes: number) => formatDiskBytes(bytes, i18n.resolvedLanguage);
	const text =
		alert.safeCount > 0
			? t("agents.cleanup.bubble.safe", {
					total: size(alert.totalBytes),
					count: alert.safeCount,
					safe: size(alert.safeBytes),
				})
			: t("agents.cleanup.bubble.noneSafe", { total: size(alert.totalBytes) });
	const cleaning = alert.progress !== null;
	return (
		<Thought agent={agent} text={text} busy={cleaning} onDismiss={offer.onDismiss}>
			<Button type="button" size="sm" variant="outline" disabled={cleaning} onClick={offer.onReview}>
				<span>{t("agents.cleanup.bubble.review")}</span>
			</Button>
			{alert.safeCount > 0 && (
				<Button type="button" size="sm" disabled={cleaning} onClick={offer.onCleanSafe}>
					{alert.progress ? (
						<>
							<Loader2 className="size-3.5 animate-spin" aria-hidden />
							<span>
								{alert.progress.total === 0
									? t("agents.cleanup.bubble.checking")
									: t("agents.cleanup.bubble.cleaning", alert.progress)}
							</span>
						</>
					) : (
						<span>{t("agents.cleanup.bubble.cleanSafe")}</span>
					)}
				</Button>
			)}
		</Thought>
	);
}

/** The balloon over the mascot: dots, the text written out, then the actions and a close. */
function Thought({
	agent,
	text,
	busy = false,
	onDismiss,
	children,
}: {
	agent: AgentView;
	text: string;
	busy?: boolean;
	onDismiss: () => void;
	children: ReactNode;
}) {
	const { t } = useTranslation("common");
	const { typed, thinking, ready } = useThoughtTyping(text);
	return (
		<div className="dcc-agent-offer max-w-[36rem] rounded-[20px] border border-border/70 bg-card px-4 py-2 shadow-sm">
			{/* Read once, in full, instead of letter by letter. */}
			<p className="sr-only" aria-live="polite">
				{agent.name}: {text}
			</p>
			{thinking ? (
				<span className="dcc-agent-thinking flex items-center gap-1 py-2" aria-hidden>
					<span />
					<span />
					<span />
				</span>
			) : (
				<p className="py-1 text-[13px] leading-relaxed text-foreground" aria-hidden>
					<span className="font-medium">{agent.name}:</span> {text.slice(0, typed)}
					{!ready && <span className="dcc-agent-caret" />}
				</p>
			)}
			{ready && (
				<span className="dcc-agent-offer-actions flex flex-wrap items-center justify-end gap-1 pb-0.5 pt-1">
					{children}
					<Button
						type="button"
						variant="ghost"
						size="icon-sm"
						aria-label={t("agents.offer.dismiss")}
						disabled={busy}
						onClick={onDismiss}
					>
						<X className="size-3.5" />
					</Button>
				</span>
			)}
		</div>
	);
}
