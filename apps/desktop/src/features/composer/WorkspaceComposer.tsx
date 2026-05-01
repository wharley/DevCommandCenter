import { useMemo, useState } from "react";
import { ArrowUp, Loader2, Square } from "lucide-react";
import { LexicalComposer } from "@lexical/react/LexicalComposer";
import { ContentEditable } from "@lexical/react/LexicalContentEditable";
import { HistoryPlugin } from "@lexical/react/LexicalHistoryPlugin";
import { LexicalErrorBoundary } from "@lexical/react/LexicalErrorBoundary";
import { PlainTextPlugin } from "@lexical/react/LexicalPlainTextPlugin";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import type { ProviderCatalog } from "@dcc/contracts";
import type { RuntimeSessionSnapshot } from "@/features/sessions/workbench-types";
import { ComposerButton } from "./ComposerButton";
import { getComposerDraftKey, canSendPrompt } from "./WorkspaceComposer.logic";
import { AutoResizePlugin } from "./editor/plugins/AutoResizePlugin";
import { DraftPersistencePlugin } from "./editor/plugins/DraftPersistencePlugin";
import { EditablePlugin } from "./editor/plugins/EditablePlugin";
import { HasContentPlugin } from "./editor/plugins/HasContentPlugin";
import { SubmitPlugin } from "./editor/plugins/SubmitPlugin";

const initialConfig = {
	namespace: "WorkspaceComposer",
	editable: true,
	onError(error: Error) {
		throw error;
	},
	nodes: [],
	theme: {
		paragraph: "min-h-[1.25rem]",
	},
};

type WorkspaceComposerProps = {
	draftKey: string;
	disabled: boolean;
	providerChoices: ProviderCatalog["providers"];
	selectedProviderId: string | null;
	selectedProviderLabel: string | null;
	sessionSnapshot: RuntimeSessionSnapshot | null;
	onSelectProvider: (providerId: string) => void;
	onStartSession: () => void;
	onSubmitPrompt: (prompt: string) => Promise<void>;
	onResumeSession: () => void;
	onAbortSession: () => void;
};

export function WorkspaceComposer({
	draftKey,
	disabled,
	providerChoices,
	selectedProviderId,
	selectedProviderLabel,
	sessionSnapshot,
	onSelectProvider,
	onStartSession,
	onSubmitPrompt,
	onResumeSession,
	onAbortSession,
}: WorkspaceComposerProps) {
	const [hasContent, setHasContent] = useState(false);
	const [isSubmitting, setIsSubmitting] = useState(false);
	const [submitAction, setSubmitAction] = useState<(() => void) | null>(null);
	const composerDraftKey = useMemo(() => getComposerDraftKey(draftKey), [draftKey]);
	const inputDisabled = disabled || isSubmitting;
	const hasProvider = Boolean(selectedProviderId);
	const canSubmit = canSendPrompt({
		disabled: inputDisabled || !hasProvider,
		hasContent,
		isSubmitting,
	});

	return (
		<div
			aria-label="Workspace composer"
			data-focus-scope="composer"
			className={cn(
				"relative flex flex-col rounded-2xl border border-border/40 bg-sidebar shadow-[0_-1px_8px_rgba(0,0,0,0.05),0_0_0_1px_rgba(255,255,255,0.02)]",
				inputDisabled ? "p-0" : "px-4 pb-3 pt-3",
				inputDisabled && "cursor-not-allowed opacity-60",
			)}
		>
			<div className="mb-2.5 flex flex-wrap items-center gap-2">
				<span className="shrink-0 pt-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
					Model
				</span>
				<div className="flex min-w-0 flex-wrap gap-1.5">
					{providerChoices.map((provider) => (
						<ComposerButton
							key={provider.id}
							active={provider.id === selectedProviderId}
							className="h-7 px-2.5 text-[11px]"
							onClick={() => onSelectProvider(provider.id)}
						>
							{provider.label}
						</ComposerButton>
					))}
				</div>
				{providerChoices.length === 0 ? (
					<span className="truncate text-[11px] text-muted-foreground">
						No providers configured
					</span>
				) : selectedProviderLabel ? (
					<span className="truncate text-[11px] text-muted-foreground">
						{selectedProviderLabel}
					</span>
				) : null}
			</div>

			<LexicalComposer initialConfig={initialConfig}>
				<div className="relative">
					<PlainTextPlugin
						contentEditable={
							<ContentEditable
								id="workspace-input"
								className={cn(
									"composer-editor min-h-[64px] max-h-[240px] resize-none overflow-x-hidden overflow-y-auto whitespace-pre-wrap break-words bg-transparent text-[14px] leading-5 tracking-[-0.01em] text-foreground outline-none",
								)}
							/>
						}
						placeholder={
							<div className="pointer-events-none absolute left-0 top-0 text-[14px] leading-5 tracking-[-0.01em] text-muted-foreground/70">
								Ask to make changes, @mention files, run /commands
							</div>
						}
						ErrorBoundary={LexicalErrorBoundary}
					/>
					<div className="pointer-events-none absolute right-0 top-0 hidden text-[11px] text-muted-foreground/60 sm:block">
						⌘Enter to send
					</div>
				</div>
				<HistoryPlugin />
				<SubmitPlugin
					draftKey={composerDraftKey}
					isDisabled={!canSubmit}
					onSubmittingChange={setIsSubmitting}
					onSubmit={onSubmitPrompt}
					registerSubmit={setSubmitAction}
				/>
				<AutoResizePlugin />
				<EditablePlugin disabled={inputDisabled} />
				<DraftPersistencePlugin draftKey={composerDraftKey} />
				<HasContentPlugin onChange={setHasContent} />
			</LexicalComposer>

			<div className="mt-2.5 flex items-end justify-between gap-3">
				<div className="flex flex-wrap items-center gap-2">
					{sessionSnapshot ? (
						<Badge variant="outline" className="h-7 px-2 text-[11px] font-normal">
							{sessionSnapshot.state}
						</Badge>
					) : (
						<Badge variant="outline" className="h-7 px-2 text-[11px] font-normal">
							idle
						</Badge>
					)}
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="h-8 px-2.5 text-xs font-normal"
						onClick={onStartSession}
						disabled={!selectedProviderId}
					>
						Start session
					</Button>
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="h-8 px-2.5 text-xs font-normal"
						onClick={onResumeSession}
						disabled={!sessionSnapshot}
					>
						Resume
					</Button>
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="h-8 px-2.5 text-xs font-normal"
						onClick={onAbortSession}
						disabled={!sessionSnapshot}
					>
						Abort
					</Button>
				</div>

				<div className="flex items-center gap-1">
					<Button
						type="button"
						variant="outline"
						size="icon"
						className="ml-1.5 rounded-[9px]"
						disabled={!canSubmit}
						onClick={() => {
							if (submitAction) {
								submitAction();
							}
						}}
					>
						<ArrowUp className="size-[15px]" />
					</Button>
					<Button
						type="button"
						variant="destructive"
						size="icon"
						className="rounded-[9px]"
						disabled={!sessionSnapshot || isSubmitting}
						onClick={onAbortSession}
					>
						<Square className="size-3 fill-current" />
					</Button>
					<div className="hidden items-center gap-1 sm:flex">
						<Button
							type="button"
							variant="default"
							size="icon"
							className="rounded-[9px]"
							disabled={!canSubmit}
							onClick={async () => {
								if (submitAction) {
									submitAction();
								}
							}}
						>
							{isSubmitting ? (
								<Loader2 className="size-[15px] animate-spin" />
							) : (
								<ArrowUp className="size-[15px]" />
							)}
						</Button>
					</div>
				</div>
			</div>
		</div>
	);
}
