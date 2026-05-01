import { useEffect, useMemo, useState } from "react";
import { ArrowUp, ArrowUpRight, Square } from "lucide-react";
import { LexicalComposer } from "@lexical/react/LexicalComposer";
import { ContentEditable } from "@lexical/react/LexicalContentEditable";
import { HistoryPlugin } from "@lexical/react/LexicalHistoryPlugin";
import { LexicalErrorBoundary } from "@lexical/react/LexicalErrorBoundary";
import { PlainTextPlugin } from "@lexical/react/LexicalPlainTextPlugin";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { DccThinkingIndicator } from "@/components/DccThinkingIndicator";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { cn } from "@/lib/utils";
import type { ProviderCatalog } from "@dcc/contracts";
import type { RuntimeSessionSnapshot } from "@/features/sessions/workbench-types";
import { ContextBar } from "./ContextBar";
import {
	buildComposerContextDirectories,
	canSendPrompt,
	decideSend,
	getComposerDraftKey,
} from "./WorkspaceComposer.logic";
import { FastModeLottieIcon } from "./FastModeLottieIcon";
import { AutoResizePlugin } from "./editor/plugins/AutoResizePlugin";
import { DraftPersistencePlugin } from "./editor/plugins/DraftPersistencePlugin";
import { EditablePlugin } from "./editor/plugins/EditablePlugin";
import { HasContentPlugin } from "./editor/plugins/HasContentPlugin";
import { SubmitPlugin } from "./editor/plugins/SubmitPlugin";
import { UsageStatsIndicator } from "./UsageStatsIndicator";
import { ProviderSelectionPanel } from "@/features/providers/provider-selection-panel";

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
	selectedModelId: string | null;
	sessionSnapshot: RuntimeSessionSnapshot | null;
	workspacePath: string | null;
	workspaceBranch: string | null;
	onSelectProvider: (providerId: string) => void;
	onSelectModel: (modelId: string) => void;
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
	selectedModelId,
	sessionSnapshot,
	workspacePath,
	workspaceBranch,
	onSelectProvider,
	onSelectModel,
	onStartSession,
	onSubmitPrompt,
	onResumeSession,
	onAbortSession,
}: WorkspaceComposerProps) {
	const [hasContent, setHasContent] = useState(false);
	const [isSubmitting, setIsSubmitting] = useState(false);
	const [submitAction, setSubmitAction] = useState<(() => void) | null>(null);
	const [isFastMode, setIsFastMode] = useState(true);
	const [effort, setEffort] = useState<"low" | "balanced" | "high">("balanced");
	const [isPlanMode, setIsPlanMode] = useState(false);
	const [contextDirectories, setContextDirectories] = useState(() =>
		buildComposerContextDirectories({ workspacePath, workspaceBranch }),
	);
	const composerDraftKey = useMemo(() => getComposerDraftKey(draftKey), [draftKey]);
	const inputDisabled = disabled || isSubmitting;
	const hasProvider = Boolean(selectedProviderId);
	const canSubmit = canSendPrompt({
		disabled: inputDisabled || !hasProvider,
		hasContent,
		isSubmitting,
	});
	const sendDecision = decideSend({
		hasContent,
		sending: isSubmitting,
		disabled: inputDisabled || !hasProvider,
	});

	useEffect(() => {
		setContextDirectories(
			buildComposerContextDirectories({ workspacePath, workspaceBranch }),
		);
	}, [workspaceBranch, workspacePath]);

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
			<ContextBar
				directories={contextDirectories}
				disabled={inputDisabled}
				onRemove={(directoryId) => {
					setContextDirectories((current) =>
						current.filter((directory) => directory.id !== directoryId),
					);
				}}
			/>

			<ProviderSelectionPanel
				title="Provider"
				description="Choose the runtime engine for this workspace."
				providers={providerChoices}
				selectedProviderId={selectedProviderId}
				selectedModelId={selectedModelId}
				onSelectProvider={onSelectProvider}
				onSelectModel={onSelectModel}
				compact
				className="mb-2.5"
			/>

			<div className="mb-2.5 flex flex-wrap items-center justify-between gap-2">
				<div className="flex flex-wrap items-center gap-1.5">
					<Button
						type="button"
						variant={isFastMode ? "default" : "outline"}
						size="sm"
						className="h-8 gap-1.5 rounded-[9px] px-2.5 text-[12px]"
						onClick={() => setIsFastMode((current) => !current)}
					>
						<FastModeLottieIcon />
						Fast
					</Button>
					<ToggleGroup
						type="single"
						value={effort}
						onValueChange={(value) => {
							if (value === "low" || value === "balanced" || value === "high") {
								setEffort(value);
							}
						}}
						className="gap-1"
					>
						{([
							["low", "Low"],
							["balanced", "Balanced"],
							["high", "High"],
						] as const).map(([value, label]) => (
							<ToggleGroupItem
								key={value}
								value={value}
								className="h-8 rounded-[9px] border border-border/60 px-2.5 text-[12px]"
							>
								{label}
							</ToggleGroupItem>
						))}
					</ToggleGroup>
					<Button
						type="button"
						variant={isPlanMode ? "default" : "outline"}
						size="sm"
						className="h-8 rounded-[9px] px-2.5 text-[12px]"
						onClick={() => setIsPlanMode((current) => !current)}
					>
						Plan
					</Button>
				</div>
				<div className="text-[11px] text-muted-foreground">
					{isFastMode ? "fast" : "normal"} · {effort} · {isPlanMode ? "plan" : "chat"}
				</div>
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

				<div className="flex items-center gap-2">
					<UsageStatsIndicator
						turnCount={sessionSnapshot?.turnCount ?? 0}
						checkpointCount={sessionSnapshot?.checkpointCount ?? 0}
						disabled={!sessionSnapshot}
					/>
					<div className="flex items-center gap-1">
						<Button
							type="button"
							variant="outline"
							size="icon"
							className="ml-1.5 rounded-[9px]"
							disabled={sendDecision.kind !== "send"}
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
							variant="outline"
							size="icon"
							className="rounded-[9px]"
							disabled={!sessionSnapshot}
							onClick={onStartSession}
							aria-label="Steer session"
						>
							<ArrowUpRight className="size-[15px]" />
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
								disabled={sendDecision.kind !== "send"}
								onClick={async () => {
									if (submitAction) {
										submitAction();
									}
								}}
							>
								{isSubmitting ? (
									<DccThinkingIndicator size={15} />
								) : (
									<ArrowUp className="size-[15px]" />
								)}
							</Button>
						</div>
					</div>
				</div>
			</div>
		</div>
	);
}
