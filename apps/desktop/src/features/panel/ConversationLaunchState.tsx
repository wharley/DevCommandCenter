import { ArrowRight, Plus, Sparkles } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	composerTurnFromRaw,
	type ComposerSubmittedTurn,
} from "@/features/composer/composer-turn";

const SUGGESTED_PROMPT_KEYS = [
	"conversationLaunch.prompts.summarize",
	"conversationLaunch.prompts.entryPoints",
	"conversationLaunch.prompts.nextAction",
] as const;

type ConversationLaunchStateProps = {
	workspaceName: string;
	selectedProviderLabel: string | null;
	selectedModelLabel: string | null;
	onStartSession: () => void;
	onSubmitPrompt: (turn: ComposerSubmittedTurn) => Promise<void>;
};

export function ConversationLaunchState({
	workspaceName,
	selectedProviderLabel,
	selectedModelLabel,
	onStartSession,
	onSubmitPrompt,
}: ConversationLaunchStateProps) {
	const { t } = useTranslation("common");
	const suggestedPrompts = useMemo(
		() => SUGGESTED_PROMPT_KEYS.map((key) => ({ key, text: t(key) })),
		[t],
	);

	return (
		<div className="flex min-h-full flex-1 items-center justify-center px-6 py-10">
			<div className="w-full max-w-2xl rounded-3xl border border-border/60 bg-card/80 p-6 shadow-[0_24px_80px_rgba(0,0,0,0.06)] backdrop-blur-sm sm:p-8">
				<div className="flex items-start justify-between gap-4">
					<div className="min-w-0">
						<div className="mb-2 inline-flex items-center gap-2 rounded-full border border-border/60 bg-muted/40 px-3 py-1 text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
							<Sparkles className="size-3.5" aria-hidden />
							<span>{t("conversationLaunch.badge")}</span>
						</div>
						<h3 className="text-[20px] font-semibold tracking-[-0.03em] text-foreground">
							{workspaceName}
						</h3>
						<p className="mt-2 max-w-xl text-[13px] leading-6 text-muted-foreground">
							{t("conversationLaunch.hint")}
						</p>
					</div>
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="shrink-0 rounded-full"
						onClick={onStartSession}
					>
						<Plus className="size-3.5" aria-hidden />
						{t("conversationLaunch.startSession")}
					</Button>
				</div>

				<div className="mt-5 flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
					<span className="rounded-full border border-border/50 bg-background/70 px-2.5 py-1">
						{t("conversationLaunch.provider")}
					</span>
					<span className="rounded-full border border-border/50 bg-background/70 px-2.5 py-1 text-foreground">
						{selectedProviderLabel ?? t("conversationLaunch.selectProvider")}
					</span>
					<span className="rounded-full border border-border/50 bg-background/70 px-2.5 py-1">
						{t("conversationLaunch.model")}
					</span>
					<span className="rounded-full border border-border/50 bg-background/70 px-2.5 py-1 text-foreground">
						{selectedModelLabel ?? t("conversationLaunch.selectModel")}
					</span>
				</div>

				<div className="mt-5 grid gap-2">
					{suggestedPrompts.map(({ key, text }) => (
						<Button
							key={key}
							type="button"
							variant="outline"
							className="h-auto justify-start rounded-2xl border-border/60 px-4 py-3 text-left"
							onClick={() => {
								void onSubmitPrompt(composerTurnFromRaw(text));
							}}
						>
							<div className="flex min-w-0 items-center gap-2">
								<ArrowRight className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
								<span className="min-w-0 text-[13px] leading-5 text-foreground">
									{text}
								</span>
							</div>
						</Button>
					))}
				</div>

				<p className="mt-4 text-[11px] leading-6 text-muted-foreground">
					{t("conversationLaunch.footerHint")}
				</p>
			</div>
		</div>
	);
}
