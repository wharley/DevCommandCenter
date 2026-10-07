import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WorkspaceComposer } from "./WorkspaceComposer";
import { FALLBACK_PROVIDER_CATALOG } from "@/lib/fallback-provider-catalog";
import { TooltipProvider } from "@/components/ui/tooltip";
import { saveDraft } from "./draftStorage";
import { getComposerConversationDraftKey } from "./WorkspaceComposer.logic";
import type { RuntimeSessionSnapshot } from "@/features/sessions/workbench-types";

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock("./ExecutionContextRail", () => ({ ExecutionContextRail: () => null }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@/lib/session-api", async (importOriginal) => ({
	...(await importOriginal<typeof import("@/lib/session-api")>()),
	loadTurnQueue: vi.fn(async () => []),
}));
const toastInfo = vi.hoisted(() => vi.fn());
vi.mock("sonner", () => ({ toast: { info: toastInfo, error: vi.fn(), success: vi.fn() } }));

const guidance = "Use the existing helper instead";
const providers = FALLBACK_PROVIDER_CATALOG.providers.map((provider) =>
	provider.id === "claude_code"
		? { ...provider, capabilities: { ...provider.capabilities, supportsSteering: true } }
		: provider,
);
const snapshot = {
	sessionId: "steered-session",
	activeTurnId: "turn-1",
	state: "active",
	turnCount: 1,
} as unknown as RuntimeSessionSnapshot;
let root: Root;
let container: HTMLDivElement;
let client: QueryClient;

function renderComposer(onSteerPrompt: () => Promise<void>, onQueuePrompt: (turn: { rawPrompt: string }) => Promise<void>) {
	return <QueryClientProvider client={client}><TooltipProvider><WorkspaceComposer
		draftKey="steer-task" draftSessionId="steered-session" disabled={false}
		providerChoices={providers}
		selectedProviderId="claude_code" selectedModelId={null} selectedProviderRuntime={null}
		sessionSnapshot={snapshot} turnQueueEventKey={null} pendingPrompt={null}
		prefill={null} onPrefillApplied={() => {}}
		workspacePath={null} workspaceBranch={null} projectLabel={null} currentBranch={null}
		isIsolatedWorkspace={true} showPlanFollowUpPrompt={false} planTitle={null}
		planNeedsInput={false} planApproved={false} onSelectProvider={() => {}} onSelectModel={() => {}}
		onAbortSession={() => {}} onReviewPlan={() => {}}
		onSubmitPrompt={async () => true}
		onSteerPrompt={onSteerPrompt}
		onQueuePrompt={onQueuePrompt}
	/></TooltipProvider></QueryClientProvider>;
}

beforeEach(() => {
	vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
	vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
	Object.defineProperty(Range.prototype, "getBoundingClientRect", { configurable: true, value: () => document.body.getBoundingClientRect() });
	Object.defineProperty(Range.prototype, "getClientRects", { configurable: true, value: () => [] });
	localStorage.clear();
	toastInfo.mockClear();
	saveDraft(getComposerConversationDraftKey("steer-task", "steered-session"), guidance);
	client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	container = document.createElement("div");
	document.body.append(container);
	root = createRoot(container);
});

afterEach(async () => {
	await act(async () => root.unmount());
	client.clear();
	container.remove();
	vi.unstubAllGlobals();
	Reflect.deleteProperty(Range.prototype, "getBoundingClientRect");
	Reflect.deleteProperty(Range.prototype, "getClientRects");
});

async function steer() {
	const button = container.querySelector<HTMLButtonElement>('[aria-label="composer.controls.steer"]');
	expect(button).not.toBeNull();
	await act(async () => button!.click());
}

it("queues guidance the provider refused because the turn was already finishing", async () => {
	const queued = vi.fn(async (_turn: { rawPrompt: string }) => {});
	await act(async () => root.render(renderComposer(
		async () => { throw new Error("provider error: steer_window_closed: Claude Code is already finishing this turn"); },
		queued,
	)));
	await steer();
	expect(queued).toHaveBeenCalledOnce();
	expect(queued.mock.calls[0][0].rawPrompt).toBe(guidance);
	expect(toastInfo).toHaveBeenCalledWith("composer.followUp.steerQueued");
	expect(container.querySelector<HTMLElement>("#workspace-input")!.textContent).toBe("");
});

it("keeps other steer failures as failures", async () => {
	const queued = vi.fn(async () => {});
	await act(async () => root.render(renderComposer(
		async () => { throw new Error("provider error: Claude Code stopped before taking the guidance"); },
		queued,
	)));
	await steer();
	expect(queued).not.toHaveBeenCalled();
	expect(container.querySelector<HTMLElement>("#workspace-input")!.textContent).toBe(guidance);
});
