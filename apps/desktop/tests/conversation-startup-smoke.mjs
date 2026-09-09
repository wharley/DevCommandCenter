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
	await button("creating").click();
	await page.getByRole("status").getByText("Preparando a conversa…", { exact: true }).waitFor();
	const initialCard = await page.getByRole("status").elementHandle();
	const initialPosition = await initialCard.boundingBox();
	for (const [stage, title] of [
		["creating", "Preparando a conversa…"],
		["catalog", "Enviando sua mensagem…"],
		["hydrating", "Enviando sua mensagem…"],
		["sending", "Enviando sua mensagem…"],
		["accepted", "Aguardando o agente…"],
	]) {
		await button(stage).click();
		await page.getByRole("status").getByText(title, { exact: true }).waitFor();
		assert.equal(await page.locator('[data-startup-state="active"]').count(), 1);
		assert.equal(await page.locator('[data-startup-state="complete"]').count(), stage === "creating" ? 0 : stage === "accepted" ? 2 : 1);
		assert.equal(await page.locator('[data-message-role="user"]').count(), 0);
		assert.equal(await page.getByText(prompt, { exact: true }).count(), 0);
		assert.equal(await page.getByRole("heading", { name: "Vamos construir", exact: true }).count(), 0);
		assert.equal(await initialCard.evaluate(element => element.isConnected), true, "preparation card stays mounted across startup phases");
		assert.equal((await initialCard.boundingBox()).y, initialPosition.y, "preparation stays in the same position");
		assert.equal(await page.getByText(/session\.started|b912712a|Session loaded/).count(), 0);
	}
	await mkdir(output, { recursive: true });
	await page.screenshot({ path: `${output}/waiting-light.png`, animations: "disabled" });
	await button("Tema").click();
	await page.setViewportSize({ width: 390, height: 780 });
	await page.screenshot({ path: `${output}/waiting-dark-mobile.png`, animations: "disabled" });
	assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
	await button("Idioma").click();
	await page.getByRole("status").getByText("Waiting for the agent…", { exact: true }).waitFor();
	await button("Idioma").click();
	await button("activity").click();
	await page.getByText("Vou conferir a autenticação e a restauração das sessões.", { exact: true }).waitFor();
	assert.equal(await page.getByRole("status").count(), 0);
	assert.equal(await page.locator('[data-message-role="user"]').count(), 1);
	assert.equal(await page.getByText(prompt, { exact: true }).count(), 1);
	assert.equal(await page.locator(".dcc-conversation-scroll-viewport").evaluate(element => getComputedStyle(element).animationName), "none");
	await page.screenshot({ path: `${output}/first-activity-dark-mobile.png`, animations: "disabled" });
	await button("idle").click();
	assert.equal(await page.getByRole("status").count(), 0);
	assert.equal(await page.getByText(/session\.started|b912712a|Session loaded/).count(), 0);
	assert.deepEqual(errors, []);
	console.log("Conversation startup smoke passed: launch to preparation, early catalog update, stable steps, deferred first prompt, translations, first activity, empty session, responsive layout.");
} finally {
	await browser.close();
}
