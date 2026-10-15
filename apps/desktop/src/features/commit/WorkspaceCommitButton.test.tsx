import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WorkspaceCommitButton } from "./WorkspaceCommitButton";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

let root: Root;
let host: HTMLDivElement;

beforeEach(() => {
	(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
	host = document.createElement("div");
	document.body.append(host);
	root = createRoot(host);
});

afterEach(() => {
	act(() => root.unmount());
	host.remove();
});

it("offers a plain commit when the repository has no remote", () => {
	act(() => root.render(<WorkspaceCommitButton mode="commit-and-push" localOnly />));
	expect(host.textContent).toContain("composer.executionDock.actions.commit");
	act(() => root.render(<WorkspaceCommitButton mode="commit-and-push" />));
	expect(host.textContent).not.toContain("composer.executionDock.actions.commit");
});
