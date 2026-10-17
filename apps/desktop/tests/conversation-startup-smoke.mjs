import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
const { chromium } = await import(process.env.DCC_PLAYWRIGHT_MODULE || "playwright");
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1100, height: 780 } });
page.setDefaultTimeout(10_000);
const errors = [];
page.on("pageerror", error => errors.push(error.message));
const button = name => page.getByRole("button", { name, exact: true });
const prompt = "Revise a autenticação do projeto e preserve as sessões existentes.";
const output = process.env.DCC_SCREENSHOT_DIR || "/tmp/dcc-conversation-startup";
try {
	await page.goto(`${process.env.DCC_STARTUP_URL || "http://127.0.0.1:1433"}/tests/conversation-startup.html`);
	await page.getByRole("heading", { name: "Vamos construir", exact: true }).waitFor();
	await button("routing").click();
	const startupRow = page.locator('[data-message-role="assistant-startup"]');
	await startupRow.getByText("Escolhendo o modelo", { exact: true }).waitFor();
	// The prompt is on screen from the first frame. The sent turn then
	// scrolls to its anchor (16px from the top) and stays there.
	const userRow = await page.locator('[data-message-role="user"]').elementHandle();
	await page.waitForTimeout(800);
	const userPosition = await userRow.boundingBox();
	for (const [stage, label] of [
		["creating", "Iniciando o Codex"],
		["catalog", "Preparando o contexto"],
		["hydrating", "Preparando o contexto"],
		["sending", "Preparando o contexto"],
		["accepted", "Aguardando a primeira resposta"],
	]) {
		await button(stage).click();
		await startupRow.getByText(label, { exact: true }).waitFor();
		assert.equal(await startupRow.count(), 1);
		assert.equal(await page.locator('[data-message-role="user"]').count(), 1);
		assert.equal(await page.getByText(prompt, { exact: true }).count(), 1);
		assert.equal(await startupRow.getByText("GPT-6 Astra", { exact: true }).count(), 1, "the reply's header is already in place");
		assert.match(await startupRow.locator(".dcc-turn-clock").textContent(), /\d+s/);
		assert.equal(await page.getByRole("heading", { name: "Vamos construir", exact: true }).count(), 0);
		assert.equal(await userRow.evaluate(element => element.isConnected), true, `prompt stays mounted at ${stage}`);
		assert.equal((await userRow.boundingBox()).y, userPosition.y, `prompt stays in place at ${stage}`);
		assert.equal(await page.getByText(/session\.started|b912712a|Session loaded/).count(), 0);
	}
	await mkdir(output, { recursive: true });
	await page.screenshot({ path: `${output}/waiting-light.png`, animations: "disabled" });
	await button("Tema").click();
	await page.setViewportSize({ width: 390, height: 780 });
	await page.screenshot({ path: `${output}/waiting-dark-mobile.png`, animations: "disabled" });
	assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
	await button("Idioma").click();
	await startupRow.getByText("Waiting for the first response", { exact: true }).waitFor();
	await button("Idioma").click();
	// The agent's first tool call takes over from the row, before any text.
	await button("tools").click();
	await page.getByText("rg -n authorized src").first().waitFor();
	assert.equal(await startupRow.count(), 0, "the agent's work replaces the startup row");
	assert.equal(await page.locator('[data-message-role="user"]').count(), 1, "the running turn's prompt is not repeated");
	assert.equal(await page.getByText(prompt, { exact: true }).count(), 1);
	await button("activity").click();
	await page.getByText("Vou conferir a autenticação e a restauração das sessões.", { exact: true }).waitFor();
	assert.equal(await startupRow.count(), 0);
	assert.equal(await page.locator('[data-message-role="user"]').count(), 1);
	assert.equal(await page.getByText(prompt, { exact: true }).count(), 1);
	assert.equal(await userRow.evaluate(element => element.isConnected), true, "accepting the prompt does not remount it");
	assert.equal(await page.locator('[data-startup-handoff="true"]').count(), 1, "the reply takes over without fading in");
	assert.equal(await page.locator(".dcc-conversation-scroll-viewport").evaluate(element => getComputedStyle(element).animationName), "none");
	await page.screenshot({ path: `${output}/first-activity-dark-mobile.png`, animations: "disabled" });
	await button("idle").click();
	assert.equal(await startupRow.count(), 0);
	assert.equal(await page.getByText(/session\.started|b912712a|Session loaded/).count(), 0);
	assert.deepEqual(errors, []);
	console.log("Conversation startup smoke passed: prompt visible from send, one live status row with real stages, continuous clock, stable prompt row, handoff without fade, translations, responsive layout.");
} finally {
	await browser.close();
}
