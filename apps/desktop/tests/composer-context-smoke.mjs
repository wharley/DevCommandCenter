// Real editor and draft persistence, synthetic IPC previews. No user files or agents.
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
const page = await browser.newPage({ viewport: { width: 1200, height: 900 } });
page.setDefaultTimeout(10000);
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
page.on("console", (message) => {
	if (message.type() === "error") errors.push(message.text());
});
const button = (name) => page.getByRole("button", { name, exact: true });
const count = async (expected) => {
	await page.waitForFunction(
		(n) =>
			document.querySelector(".composer-context-count")?.textContent ===
			String(n),
		expected,
	);
};
const expand = async () => {
	const summary = page.locator(".composer-context-summary");
	if ((await summary.getAttribute("aria-expanded")) === "false")
		await summary.click();
};
const prompt = async () => {
	await button("Enviar").click();
	return page.evaluate(() => window.contextPrompt);
};
const preview = async (path) => {
	await button(`Visualizar ${path}`).click();
	await page.getByRole("dialog").waitFor();
};
const close = async () => {
	await page.keyboard.press("Escape");
	await page.getByRole("dialog").waitFor({ state: "hidden" });
};
try {
	await page.goto(
		`${process.env.DCC_COMPOSER_URL || "http://127.0.0.1:1432"}/tests/composer-context.html`,
	);
	await button("Adicionar contexto").click();
	await count(4);
	const original = await prompt();
	await expand();
	assert.equal(await page.locator(".composer-context-item").count(), 4);
	await preview("src/auth/config.ts");
	await page
		.getByRole("dialog")
		.getByText("export const session = { persist: true, retry: 3 };")
		.waitFor();
	await close();
	await preview("design/capture.png");
	await page.waitForFunction(
		() => document.querySelector(".composer-context-image")?.naturalWidth > 0,
	);
	await close();
	await preview("Texto colado");
	await page
		.getByRole("dialog")
		.getByText("Preserve o comportamento existente.", { exact: false })
		.waitFor();
	await close();
	assert.equal(
		await prompt(),
		original,
		"preview does not change the outgoing message",
	);
	await button("Remover src/auth/config.ts do rascunho").click();
	await count(3);
	assert.equal(await prompt(), original.replace("@src/auth/config.ts", ""));
	await button("Desfazer").click();
	await count(4);
	assert.equal(await prompt(), original, "undo restores the exact prompt");
	await button("Trocar conversa").click();
	await page.locator(".composer-context-review").waitFor({ state: "hidden" });
	assert.equal(await prompt(), "");
	await button("Trocar conversa").click();
	await count(4);
	assert.equal(await prompt(), original);
	await page.reload();
	await count(4);
	assert.equal(await prompt(), original, "reload preserves structured context");
	await expand();
	await button("Simular falha").click();
	await preview("src/auth/config.ts");
	await button("Tentar novamente").waitFor();
	await page.evaluate(() => {
		window.previewFailure = false;
	});
	await button("Tentar novamente").click();
	await page.locator(".composer-context-text").waitFor();
	await close();
	const sendsBefore = await page.evaluate(() => window.contextSendCount);
	const inlineRemove = page
		.getByRole("textbox", { name: "Mensagem de teste" })
		.getByRole("button", { name: /Remover config.ts do rascunho/ })
		.first();
	await inlineRemove.focus();
	await page.keyboard.press("Enter");
	await count(3);
	assert.equal(
		await page.evaluate(() => window.contextSendCount),
		sendsBefore,
		"Enter on inline removal does not send",
	);
	await button("Adicionar contexto").click();
	await count(4);
	await expand();
	const output =
		process.env.DCC_SCREENSHOT_DIR || "/tmp/dcc-composer-context-smoke";
	await mkdir(output, { recursive: true });
	await page.screenshot({ path: `${output}/dark.png`, fullPage: true });
	await button("Trocar tema").click();
	await page.screenshot({ path: `${output}/light.png`, fullPage: true });
	await page.setViewportSize({ width: 420, height: 740 });
	await page.emulateMedia({ reducedMotion: "reduce" });
	assert.equal(
		await page.evaluate(
			() => document.documentElement.scrollWidth <= innerWidth,
		),
		true,
	);
	await preview("Texto colado");
	const bounds = await page.getByRole("dialog").boundingBox();
	assert.ok(bounds.x >= 0 && bounds.x + bounds.width <= 420);
	await page.screenshot({
		path: `${output}/compact-preview.png`,
		fullPage: true,
	});
	await close();
	assert.deepEqual(errors, []);
	console.log(
		"PASS: previews, retry, occurrence removal, undo, unchanged send text, keyboard safety, conversation isolation, reload, themes and compact dialog. Screenshots: " +
			output,
	);
} finally {
	await browser.close();
}
