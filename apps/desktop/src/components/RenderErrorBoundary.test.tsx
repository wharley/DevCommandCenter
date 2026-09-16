import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RenderErrorBoundary } from "./RenderErrorBoundary";
import { FrontendDiagnostics } from "./FrontendDiagnostics";
import { readFrontendErrors } from "@/lib/frontend-diagnostics";

let root: Root;
let container: HTMLDivElement;
beforeEach(() => {
	(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
	container = document.createElement("div");
	document.body.append(container);
	root = createRoot(container);
	vi.spyOn(console, "error").mockImplementation(() => {});
});
afterEach(async () => {
	await act(async () => root.unmount());
	container.remove();
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
});

function Answer({ crash }: { crash: boolean }) {
	if (crash) throw new Error("simulated streaming render failure");
	return <p>Resposta em andamento</p>;
}

describe("render recovery", () => {
	it("contains a streaming update failure and preserves navigation until another session is selected", async () => {
		const render = (session: string, crash: boolean) => act(async () => root.render(
			<RenderErrorBoundary scope="app">
				<nav><button>Outra tarefa</button></nav>
				<RenderErrorBoundary scope="workspace" resetKey={session} context={{ sessionId: session }}>
					<Answer crash={crash} />
				</RenderErrorBoundary>
			</RenderErrorBoundary>,
		));
		await render("session-a", false);
		expect(container.textContent).toContain("Resposta em andamento");
		await render("session-a", true);
		expect(container.querySelector("nav")?.textContent).toBe("Outra tarefa");
		expect(container.querySelector('[role="alert"]')?.textContent).toContain("Recarregar aplicativo");
		const record = readFrontendErrors().at(-1)!;
		expect(record.scope).toBe("workspace");
		expect(record.context.sessionId).toBe("session-a");
		expect(record.componentStack).toContain("Answer");
		// More stream updates must not trigger an automatic crash/retry loop.
		await render("session-a", false);
		expect(container.querySelector('[role="alert"]')).not.toBeNull();
		await render("session-b", false);
		expect(container.querySelector('[role="alert"]')).toBeNull();
		expect(container.textContent).toContain("Resposta em andamento");
	});

	it("recovers outside all app providers and copies the recorded component stack", async () => {
		const writeText = vi.fn().mockResolvedValue(undefined);
		vi.stubGlobal("navigator", { clipboard: { writeText }, userAgent: "test" });
		await act(async () => root.render(<RenderErrorBoundary scope="app"><Answer crash /></RenderErrorBoundary>));
		const copy = [...container.querySelectorAll("button")].find((button) => button.textContent === "Copiar diagnóstico")!;
		await act(async () => copy.click());
		const copied = JSON.parse(writeText.mock.calls[0][0]);
		expect(copied.errors.at(-1).scope).toBe("app");
		expect(copied.errors.at(-1).componentStack).toContain("Answer");
		expect(container.querySelector('[role="status"]')?.textContent).toBe("Diagnóstico copiado.");
	});

	it("offers a selectable report when clipboard access fails after the app is reopened", async () => {
		vi.stubGlobal("navigator", { clipboard: { writeText: vi.fn().mockRejectedValue(new Error("denied")) } });
		await act(async () => root.render(<FrontendDiagnostics />));
		await act(async () => container.querySelector("button")!.click());
		const textarea = container.querySelector("textarea")!;
		expect(textarea.readOnly).toBe(true);
		expect(JSON.parse(textarea.value).schemaVersion).toBe(1);
	});
});
