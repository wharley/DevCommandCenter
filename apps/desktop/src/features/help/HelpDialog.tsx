import { Keyboard, Search } from "lucide-react";
import { Fragment, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { InlineShortcutDisplay } from "@/features/shortcuts/InlineShortcutDisplay";
import { cn } from "@/lib/utils";
import {
	DEFAULT_HELP_TOPIC,
	HELP_TOPIC_ICONS,
	HELP_TOPIC_IDS,
	matchesHelpTopic,
	resolveHelpTopicShortcut,
	type HelpTopicId,
} from "./help-topics";

type HelpDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	/** Topic to land on when the dialog opens; contextual entry points pass this. */
	initialTopic?: HelpTopicId | null;
	onOpenShortcuts: () => void;
};

type HelpTopicCopy = {
	id: HelpTopicId;
	label: string;
	summary: string;
	whatIs: string;
	whenToUse: string;
	steps: string[];
	prompt: string | null;
	tip: string | null;
	keywords: string;
};

export function HelpDialog({
	open,
	onOpenChange,
	initialTopic,
	onOpenShortcuts,
}: HelpDialogProps) {
	const { t } = useTranslation("common");
	const [query, setQuery] = useState("");
	const [activeTopic, setActiveTopic] = useState<HelpTopicId>(
		initialTopic ?? DEFAULT_HELP_TOPIC,
	);

	useEffect(() => {
		if (open) {
			setQuery("");
			setActiveTopic(initialTopic ?? DEFAULT_HELP_TOPIC);
		}
	}, [open, initialTopic]);

	const topics = useMemo<HelpTopicCopy[]>(
		() =>
			HELP_TOPIC_IDS.map((id) => {
				const base = `help.topics.${id}` as const;
				const steps = t(`${base}.steps`, { returnObjects: true });
				const prompt = t(`${base}.prompt`, { defaultValue: "" });
				const tip = t(`${base}.tip`, { defaultValue: "" });
				return {
					id,
					label: t(`${base}.label`),
					summary: t(`${base}.summary`),
					whatIs: t(`${base}.whatIs`),
					whenToUse: t(`${base}.whenToUse`),
					steps: Array.isArray(steps) ? (steps as string[]) : [],
					prompt: prompt.length > 0 ? prompt : null,
					tip: tip.length > 0 ? tip : null,
					keywords: t(`${base}.keywords`, { defaultValue: "" }),
				};
			}),
		[t],
	);

	const visibleTopics = useMemo(
		() =>
			topics.filter((topic) =>
				matchesHelpTopic(
					`${topic.label} ${topic.summary} ${topic.keywords} ${topic.whatIs}`,
					query,
				),
			),
		[topics, query],
	);

	// Keep the card in sync with the filtered list so a search never shows a
	// topic on the right that is missing from the list on the left.
	useEffect(() => {
		if (visibleTopics.length === 0) return;
		if (!visibleTopics.some((topic) => topic.id === activeTopic)) {
			setActiveTopic(visibleTopics[0]!.id);
		}
	}, [visibleTopics, activeTopic]);

	const current =
		visibleTopics.find((topic) => topic.id === activeTopic) ??
		visibleTopics[0] ??
		null;
	const shortcut = current ? resolveHelpTopicShortcut(current.id) : null;

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="h-[min(84vh,700px)] w-[min(94vw,1040px)] overflow-hidden rounded-2xl border-border/60 bg-background p-0 shadow-2xl sm:max-w-[1040px]">
				<div className="flex h-full min-h-0 w-full min-w-0 overflow-hidden">
					<nav className="scrollbar-stable flex w-[260px] shrink-0 flex-col overflow-hidden border-r border-sidebar-border bg-sidebar py-5">
						<div className="px-4 pb-3">
							<DialogHeader>
								<DialogTitle className="text-[15px] font-semibold text-foreground">
									{t("help.title")}
								</DialogTitle>
								<DialogDescription className="sr-only">
									{t("help.description")}
								</DialogDescription>
							</DialogHeader>
							<div className="relative mt-3">
								<Search
									className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground"
									strokeWidth={1.9}
									aria-hidden
								/>
								<Input
									value={query}
									onChange={(event) => setQuery(event.target.value)}
									placeholder={t("help.searchPlaceholder")}
									aria-label={t("help.searchPlaceholder")}
									className="h-8 bg-background pl-8 text-[13px]"
								/>
							</div>
						</div>

						<div className="min-h-0 flex-1 space-y-1 overflow-y-auto px-3">
							{visibleTopics.length === 0 ? (
								<p className="px-3 py-6 text-center text-[12px] leading-5 text-muted-foreground">
									{t("help.empty")}
								</p>
							) : (
								visibleTopics.map((topic) => {
									const Icon = HELP_TOPIC_ICONS[topic.id];
									const active = current?.id === topic.id;
									return (
										<button
											key={topic.id}
											type="button"
											onClick={() => setActiveTopic(topic.id)}
											aria-current={active ? "true" : undefined}
											className={cn(
												"flex w-full items-start gap-3 rounded-lg px-3 py-2 text-left transition-colors",
												active
													? "bg-accent/70 text-foreground"
													: "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
											)}
										>
											<Icon className="mt-0.5 size-4 shrink-0" strokeWidth={1.9} aria-hidden />
											<div className="min-w-0 flex-1">
												<div className="text-[13px] font-medium leading-tight">{topic.label}</div>
												<p className="mt-0.5 text-[11px] leading-tight text-muted-foreground/80">
													{topic.summary}
												</p>
											</div>
										</button>
									);
								})
							)}
						</div>

						<div className="mt-auto border-t border-sidebar-border px-3 pt-3">
							<Button
								type="button"
								variant="ghost"
								size="sm"
								className="w-full justify-start gap-2.5 text-muted-foreground hover:text-foreground"
								onClick={() => {
									onOpenChange(false);
									onOpenShortcuts();
								}}
							>
								<Keyboard className="size-4" strokeWidth={1.9} aria-hidden />
								{t("help.actions.openShortcuts")}
							</Button>
						</div>
					</nav>

					<div className="flex min-w-0 flex-1 flex-col overflow-hidden">
						{current ? (
							<>
								<div className="flex items-center justify-between gap-4 border-b border-border/40 px-6 py-4 lg:px-8">
									<div className="min-w-0">
										<h2 className="text-[15px] font-semibold text-foreground">{current.label}</h2>
										<p className="mt-0.5 text-[12px] text-muted-foreground">{current.summary}</p>
									</div>
									{shortcut ? (
										<div className="flex shrink-0 items-center gap-2 text-[11px] text-muted-foreground">
											<span>{t("help.sections.shortcut")}</span>
											<InlineShortcutDisplay keys={[...shortcut]} />
										</div>
									) : null}
								</div>

								<div className="min-w-0 flex-1 overflow-x-hidden overflow-y-auto px-6 pt-5 pb-6 lg:px-8">
									<div className="mx-auto flex max-w-[640px] flex-col gap-6">
										<HelpSection title={t("help.sections.whatIs")}>
											<p className="text-[13px] leading-6 text-foreground/90">
												<HelpRichText text={current.whatIs} />
											</p>
										</HelpSection>

										<HelpSection title={t("help.sections.whenToUse")}>
											<p className="text-[13px] leading-6 text-foreground/90">
												<HelpRichText text={current.whenToUse} />
											</p>
										</HelpSection>

										{current.steps.length > 0 ? (
											<HelpSection title={t("help.sections.steps")}>
												<ol className="flex flex-col gap-2">
													{current.steps.map((step, index) => (
														<li
															key={`${current.id}-${index}`}
															className="flex items-start gap-3 rounded-xl border border-border/60 px-4 py-3"
														>
															<span className="mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full bg-accent text-[11px] font-medium tabular-nums text-foreground">
																{index + 1}
															</span>
															<span className="text-[13px] leading-6 text-foreground/90">
																<HelpRichText text={step} />
															</span>
														</li>
													))}
												</ol>
											</HelpSection>
										) : null}

										{current.prompt ? (
											<HelpSection title={t("help.sections.prompt")}>
												<pre className="whitespace-pre-wrap rounded-xl border border-border/60 bg-muted/30 px-4 py-3 font-mono text-[12px] leading-6 text-foreground/90">
													{current.prompt}
												</pre>
											</HelpSection>
										) : null}

										{current.tip ? (
											<p className="rounded-xl border border-dashed border-border/60 px-4 py-3 text-[12px] leading-5 text-muted-foreground">
												<HelpRichText text={current.tip} />
											</p>
										) : null}
									</div>
								</div>
							</>
						) : (
							<div className="flex flex-1 items-center justify-center px-6 text-center text-[13px] leading-6 text-muted-foreground">
								{t("help.emptyHint")}
							</div>
						)}
					</div>
				</div>
			</DialogContent>
		</Dialog>
	);
}

function HelpSection({ title, children }: { title: string; children: React.ReactNode }) {
	return (
		<section className="flex flex-col gap-2">
			<h3 className="text-[12px] font-medium text-muted-foreground">{title}</h3>
			{children}
		</section>
	);
}

const UI_LABEL_PATTERN = /\[([^\]]+)\]/g;

/**
 * Renders `[Rótulo]` fragments as small chips so a person can match the text
 * to the exact control they see in the app. Everything else is plain text.
 */
function HelpRichText({ text }: { text: string }) {
	const parts = useMemo(() => {
		const output: Array<{ kind: "text" | "label"; value: string }> = [];
		let lastIndex = 0;
		for (const match of text.matchAll(UI_LABEL_PATTERN)) {
			const start = match.index ?? 0;
			if (start > lastIndex) {
				output.push({ kind: "text", value: text.slice(lastIndex, start) });
			}
			output.push({ kind: "label", value: match[1]! });
			lastIndex = start + match[0].length;
		}
		if (lastIndex < text.length) {
			output.push({ kind: "text", value: text.slice(lastIndex) });
		}
		return output;
	}, [text]);

	return (
		<>
			{parts.map((part, index) =>
				part.kind === "label" ? (
					<span
						key={index}
						className="mx-0.5 inline-flex items-center rounded-md border border-border/70 bg-background px-1.5 py-px align-baseline text-[12px] font-medium leading-5 text-foreground"
					>
						{part.value}
					</span>
				) : (
					<Fragment key={index}>{part.value}</Fragment>
				),
			)}
		</>
	);
}
