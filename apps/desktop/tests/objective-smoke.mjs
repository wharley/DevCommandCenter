import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
const { chromium } = await import(
	process.env.DCC_PLAYWRIGHT_MODULE || "playwright"
);
const browser = await chromium.launch({
	headless: true,
	...(process.env.DCC_CHROMIUM_EXECUTABLE
		? { executablePath: process.env.DCC_CHROMIUM_EXECUTABLE }
		: {}),
});
const page = await browser.newPage({ viewport: { width: 1100, height: 920 } });
page.setDefaultTimeout(12000);
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
const button = (name) => page.getByRole("button", { name, exact: true });
const panel = page.getByRole("dialog", {
	name: "Objetivo da tarefa",
	exact: true,
});
const output = process.env.DCC_SCREENSHOT_DIR || "/tmp/dcc-objective-smoke";
const open = async (scenario = "") => {
	await page.goto(
		`${process.env.DCC_OBJECTIVE_URL || "http://127.0.0.1:1432"}/tests/objective.html?case=${scenario}`,
	);
	await button("Objetivo da tarefa").click();
	await panel.waitFor();
};
const shot = async (name) => {
	await panel.evaluate((el) =>
		Promise.allSettled(el.getAnimations().map((a) => a.finished)),
	);
	await page.screenshot({
		path: `${output}/${name}.png`,
		animations: "disabled",
		style: "[data-sonner-toaster] { visibility: hidden; }",
	});
};
const fits = async () => {
	await page.waitForFunction(() => {
		const el = document.querySelector(".objective-panel");
		if (!el) return false;
		const r = el.getBoundingClientRect();
		return (
			r.x >= 0 &&
			r.y >= 0 &&
			r.right <= innerWidth &&
			r.bottom <= innerHeight &&
			el.scrollWidth <= el.clientWidth
		);
	});
	const footer = await page.locator(".objective-footer").boundingBox();
	const p = await panel.boundingBox();
	assert(footer.y >= p.y && footer.y + footer.height <= p.y + p.height + 1);
};
try {
	await mkdir(output, { recursive: true });
	await open("empty");
	await page.locator("#objective-intent").waitFor();
	await fits();
	await shot("empty-light");
	assert(await button("Definir objetivo").isDisabled());
	await page.locator("#objective-done-when").fill("Testes aprovados.");
	await button("Definir objetivo").click();
	await page.getByText("A intenção é obrigatória.", { exact: true }).waitFor();
	assert.equal(
		await page.evaluate(
			() =>
				window.objectiveCalls.filter(
					(c) => c.command === "set_session_objective",
				).length,
		),
		0,
	);
	await page.locator("#objective-intent").fill("Implementar a melhoria.");
	await page.locator("#objective-max-turns").fill("0");
	await button("Definir objetivo").click();
	await page.getByText(/O orçamento de turnos deve ficar/).waitFor();
	await page.locator("#objective-max-turns").fill("12");
	await button("Definir objetivo").click();
	await button("Pausar").waitFor();
	const saved = await page.evaluate(
		() =>
			window.objectiveCalls.find((c) => c.command === "set_session_objective")
				.args.input,
	);
	assert.equal(saved.sessionId, "session-demo");
	assert.equal(saved.expectedGeneration, null);
	assert.equal(saved.draft.maxTurns, 12);
	await button("Pausar").click();
	await button("Retomar").waitFor();
	await page.getByText("Pausado", { exact: true }).waitFor();
	await button("Retomar").click();
	await button("Pausar").waitFor();
	await button("Marcar concluído").click();
	await page.getByText("Concluído", { exact: true }).waitFor();
	assert.equal(await button("Pausar").count(), 0);
	await button("Retomar").click();
	await button("Pausar").waitFor();
	await page.locator("#objective-intent").fill("Objetivo revisado.");
	await button("Salvar").click();
	await page.waitForFunction(
		() =>
			document.querySelector(".objective-generation")?.textContent === "rev 6",
	);
	const writes = await page.evaluate(() =>
		window.objectiveCalls.filter((c) => c.command === "set_session_objective"),
	);
	assert.equal(writes[1].args.input.expectedGeneration, 5);
	await button("Limpar").click();
	await panel.waitFor({ state: "hidden" });
	await button("Objetivo da tarefa").click();
	await button("Definir objetivo").waitFor();
	assert.equal(await page.locator("#objective-intent").inputValue(), "");
	await page.keyboard.press("Escape");
	await panel.waitFor({ state: "hidden" });
	assert(
		await button("Objetivo da tarefa").evaluate(
			(el) => el === document.activeElement,
		),
	);
	await open();
	await button("Pausar").waitFor();
	await fits();
	await shot("active-light");
	await button("Fechar objetivo").click();
	await button("Tema").click();
	await button("Objetivo da tarefa").click();
	await fits();
	await shot("active-dark");
	await page.setViewportSize({ width: 390, height: 740 });
	await page.emulateMedia({ reducedMotion: "reduce" });
	await fits();
	await shot("active-compact");
	await button("Pausar").click();
	await button("Retomar").waitFor();
	await fits();
	await shot("paused-compact");
	await page.locator("#objective-max-turns").scrollIntoViewIfNeeded();
	await fits();
	await shot("limits-compact");
	await open("paused");
	await page.getByText("orçamento de turnos", { exact: true }).waitFor();
	await fits();
	await open("error");
	await page.getByRole("alert").waitFor();
	assert(await button("Definir objetivo").isDisabled());
	await button("Tentar novamente").click();
	await button("Pausar").waitFor();
	await open("save-error");
	await page.locator("#objective-intent").fill("Rascunho preservado.");
	await button("Salvar").click();
	await page
		.getByText("Falha de gravação de demonstração.", { exact: true })
		.waitFor();
	assert.equal(
		await page.locator("#objective-intent").inputValue(),
		"Rascunho preservado.",
	);
	assert(await button("Salvar").isEnabled());
	await open("slow");
	await page.getByText("Carregando objetivo…", { exact: true }).waitFor();
	assert(await button("Definir objetivo").isDisabled());
	await page.locator("#objective-intent").waitFor();
	assert.deepEqual(errors, []);
	console.log(
		"PASS: objective create/edit, validation, generations, pause/resume/complete/clear, loading/retry/error, keyboard, themes and compact layout. Screenshots: " +
			output,
	);
} catch (e) {
	console.error(await page.locator("body").innerText());
	await page.screenshot({ path: `${output}/failure.png` });
	throw e;
} finally {
	await browser.close();
}
