import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WorkspaceComposer } from "./WorkspaceComposer";
import { useComposerPrefill, type ExternalComposerPrefill } from "./use-composer-prefill";
import { FALLBACK_PROVIDER_CATALOG } from "@/lib/fallback-provider-catalog";
import { TooltipProvider } from "@/components/ui/tooltip";
import { loadDraft, saveDraft } from "./draftStorage";
import { getComposerConversationDraftKey } from "./WorkspaceComposer.logic";

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock("./ExecutionContextRail", () => ({ ExecutionContextRail: () => null }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const note = { text: "Analisar MCP\n\nRevisar a criação de gateways", nonce: 1 };
let root: Root;
let container: HTMLDivElement;
let client: QueryClient;
const sent = vi.fn();
let accept: (value: boolean) => void;
let prepareSession: (sessionId: string) => void;

function Harness() {
	const [external, setExternal] = useState<ExternalComposerPrefill | null>(note);
	const [sessionId, setSessionId] = useState<string | null>(null);
	const controller = useComposerPrefill({
		workspaceId: "task-from-note", selectedSessionId: sessionId,
		// App reconstructs this prop on render; preserving object identity here
		// would hide the effect that used to replay the note after acknowledgment.
		externalComposerPrefill: external ? { ...external } : null,
		onExternalComposerPrefillConsumed: () => setExternal(null),
	});
	return <WorkspaceComposer
		draftKey="task-from-note" draftSessionId={sessionId} disabled={false}
		providerChoices={FALLBACK_PROVIDER_CATALOG.providers}
		selectedProviderId="codex" selectedModelId={null} selectedProviderRuntime={null}
		sessionSnapshot={null} turnQueueEventKey={null} pendingPrompt={null}
		prefill={controller.composerPrefill} onPrefillApplied={controller.handleComposerPrefillApplied}
		workspacePath={null} workspaceBranch={null} projectLabel={null} currentBranch={null}
		isIsolatedWorkspace={true} showPlanFollowUpPrompt={false} planTitle={null}
		planNeedsInput={false} planApproved={false} onSelectProvider={() => {}} onSelectModel={() => {}}
		onAbortSession={() => {}} onReviewPlan={() => {}}
		onSubmitPrompt={async (turn) => {
			sent(turn.rawPrompt);
			await Promise.resolve();
			setSessionId("session-created");
			return new Promise<boolean>((resolve) => { accept = resolve; });
		}}
	/>;
}

function DraftTransitionHarness() {
	const [sessionId, setSessionId] = useState<string | null>(null);
	prepareSession = setSessionId;
	return <WorkspaceComposer
		draftKey="task-from-note" draftSessionId={sessionId} disabled={false}
		providerChoices={FALLBACK_PROVIDER_CATALOG.providers}
		selectedProviderId="codex" selectedModelId={null} selectedProviderRuntime={null}
		sessionSnapshot={null} turnQueueEventKey={null} pendingPrompt={null}
		prefill={null} onPrefillApplied={() => {}}
		workspacePath={null} workspaceBranch={null} projectLabel={null} currentBranch={null}
		isIsolatedWorkspace={true} showPlanFollowUpPrompt={false} planTitle={null}
		planNeedsInput={false} planApproved={false} onSelectProvider={() => {}} onSelectModel={() => {}}
		onAbortSession={() => {}} onReviewPlan={() => {}}
		onSubmitPrompt={async (turn) => {
			sent(turn.rawPrompt);
			return true;
		}}
	/>;
}

beforeEach(() => {
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
	Object.defineProperty(Range.prototype, "getBoundingClientRect", { configurable: true, value: () => document.body.getBoundingClientRect() });
	Object.defineProperty(Range.prototype, "getClientRects", { configurable: true, value: () => [] });
	localStorage.clear();
	sent.mockClear();
	client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	container = document.createElement("div");
	document.body.append(container);
	root = createRoot(container);
});

afterEach(async () => {
	await act(async () => root.unmount());
	client.clear();
	container.remove();
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
	Reflect.deleteProperty(Range.prototype, "getBoundingClientRect");
	Reflect.deleteProperty(Range.prototype, "getClientRects");
});

it.each(["click", "enter"])("clears the note draft after %s and first session creation", async (method) => {
	await act(async () => root.render(<QueryClientProvider client={client}><TooltipProvider><Harness /></TooltipProvider></QueryClientProvider>));
	const input = container.querySelector<HTMLElement>("#workspace-input")!;
	expect(input.textContent).toBe(note.text);
	await act(async () => {
		if (method === "click") container.querySelector<HTMLButtonElement>('[aria-label="composer.controls.send"]')!.click();
		else input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", code: "Enter", keyCode: 13, bubbles: true, cancelable: true }));
	});
	expect(sent).toHaveBeenCalledExactlyOnceWith(note.text);
	expect(input.textContent).toBe("");
	await act(async () => accept(true));
	expect(input.textContent).toBe("");
	expect(loadDraft(getComposerConversationDraftKey("task-from-note", null))).toBe("");
	expect(loadDraft(getComposerConversationDraftKey("task-from-note", "session-created"))).toBe("");
});

it("moves a saved new-session draft into a prepared session without submitting it", async () => {
	const draft = "Prepare the desktop app before testing it";
	const fallbackKey = getComposerConversationDraftKey("task-from-note", null);
	const sessionKey = getComposerConversationDraftKey("task-from-note", "prepared-session");
	saveDraft(fallbackKey, draft);

	await act(async () => root.render(<QueryClientProvider client={client}><TooltipProvider><DraftTransitionHarness /></TooltipProvider></QueryClientProvider>));
	const input = container.querySelector<HTMLElement>("#workspace-input")!;
	expect(input.textContent).toBe(draft);

	await act(async () => prepareSession("prepared-session"));
	expect(input.textContent).toBe(draft);
	expect(loadDraft(fallbackKey)).toBe("");
	expect(loadDraft(sessionKey)).toBe(draft);
	expect(sent).not.toHaveBeenCalled();
});
