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
const page = await browser.newPage({ viewport: { width: 1000, height: 800 } });
page.setDefaultTimeout(12000);
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
page.on("console", (message) => {
	if (message.type() === "error") errors.push(message.text());
});
const button = (name) => page.getByRole("button", { name, exact: true });
const dialog = page.getByRole("dialog", { name: "Enviar trecho ao agente" });
const output =
	process.env.DCC_SCREENSHOT_DIR || "/tmp/dcc-code-annotation-smoke";
const shot = (name) => page.screenshot({ path: `${output}/${name}.png` });
const lastAction = () => page.evaluate(() => window.annotationActions.at(-1));
const openPlus = async (selector) => {
	await page.locator(selector).hover();
	const plus = page.locator("[data-utility-button]");
	await plus.waitFor();
	assert.equal(
		await plus.evaluate((el) => getComputedStyle(el).borderRadius),
		"7px",
		"shared gutter styling crosses Pierre shadow roots",
	);
	await plus.click();
	await dialog.waitFor();
};
const withinViewport = async () => {
	const box = await dialog.boundingBox();
	const viewport = page.viewportSize();
	assert(
		box.x >= 10 &&
			box.y >= 10 &&
			box.x + box.width <= viewport.width - 10 &&
			box.y + box.height <= viewport.height - 10,
		"annotation stays inside viewport",
	);
	assert.equal(
		await dialog.evaluate((el) => el.scrollWidth <= el.clientWidth),
		true,
	);
};
try {
	await mkdir(output, { recursive: true });
	await page.goto(
		`${process.env.DCC_ANNOTATION_URL || "http://127.0.0.1:1432"}/tests/code-annotation.html`,
	);
	await openPlus('[data-line="5"][data-line-type="change-deletion"]');
	assert.equal(
		await dialog.locator("pre").textContent(),
		"export const ready = false;",
	);
	await dialog.getByText("removido", { exact: true }).waitFor();
	assert.equal(
		await dialog
			.locator("textarea")
			.evaluate((el) => document.activeElement === el),
		true,
	);
	assert.equal(await button("Enviar").isDisabled(), true);
	await dialog.locator("textarea").fill("Preserve a validação.");
	await shot("diff-annotation-light");
	await dialog.locator("textarea").press("Control+Enter");
	let action = await lastAction();
	assert.equal(action.request.side, "original");
	assert.equal(action.request.startLine, 5);
	assert.equal(action.instruction, "Preserve a validação.");
	assert.equal(action.newSession, false);
	await openPlus('[data-line="5"][data-line-type="change-addition"]');
	await dialog.locator("textarea").fill("Rever este trecho.");
	await button("Nova sessão").click();
	assert.equal((await lastAction()).newSession, true);
	await button("Arquivo").click();
	await openPlus('[data-line="2"]');
	await button("Editar no composer").click();
	action = await lastAction();
	assert.equal(action.action, "composer");
	assert.equal(action.request.startLine, 2);
	assert.equal(action.request.side, "modified");
	// Select a range using the real read-only gutter.
	const start = await page.locator('[data-column-number="1"]').boundingBox();
	const end = await page.locator('[data-column-number="3"]').boundingBox();
	await page.mouse.move(start.x + 5, start.y + start.height / 2);
	await page.mouse.down();
	await page.mouse.move(end.x + 5, end.y + end.height / 2, { steps: 5 });
	await page.mouse.up();
	await button("Comentar trecho").click();
	await dialog.locator("textarea").fill("Verificar saudação.");
	await button("Adicionar como evidência").click();
	action = await lastAction();
	assert.equal(action.action, "review");
	assert.equal(action.request.startLine, 1);
	assert.equal(action.request.endLine, 3);
	await button("Editor").click();
	await page.locator('[contenteditable="true"] [data-line="1"]').waitFor();
	const editableLine = await page.locator('[contenteditable="true"] [data-line="1"]').boundingBox();
	await page.mouse.move(editableLine.x + 20, editableLine.y + 8);
	await page.mouse.down();
	await page.mouse.move(editableLine.x + 300, editableLine.y + 8, { steps: 8 });
	await page.mouse.up();
	await button("Comentar trecho").waitFor();
	assert.equal(
		await page
			.locator("[data-selection-action-popover]")
			.evaluate((el) => getComputedStyle(el).boxShadow),
		"none",
	);
	await shot("editor-selection");
	await button("Comentar trecho").click();
	await dialog.waitFor();
	await dialog.locator("textarea").fill("Analisar a assinatura.");
	await button("Enviar").click();
	action = await lastAction();
	assert.equal(action.request.startLine, 1);
	assert(action.request.snippet.includes("function welcome"));
	// Dismiss from a footer control as well as the input; keyboard stays in the dialog.
	await openPlus('[data-line="2"]');
	await dialog.locator("textarea").fill("Um comentário.");
	await button("Enviar").focus();
	await page.keyboard.press("Tab");
	assert.equal(
		await button("Fechar anotação de código").evaluate(
			(el) => document.activeElement === el,
		),
		true,
	);
	await page.keyboard.press("Shift+Tab");
	assert.equal(
		await button("Enviar").evaluate((el) => document.activeElement === el),
		true,
	);
	await page.keyboard.press("Escape");
	assert.equal(await dialog.count(), 0);
	await page.locator('[data-line="1"]').click({ position: { x: 80, y: 8 } });
	await page.keyboard.insertText("// ");
	await openPlus('[data-line="1"]');
	assert(
		(await dialog.locator("pre").textContent()).includes("// "),
		"annotation reads the current edited content",
	);
	await button("Fechar anotação de código").click();

	await button("Tema").click();
	await button("Seleção longa").click();
	await withinViewport();
	await shot("long-annotation-dark");
	assert.equal((await dialog.locator("pre").textContent()).length, 6000);
	await page.setViewportSize({ width: 360, height: 560 });
	await page.emulateMedia({ reducedMotion: "reduce" });
	await withinViewport();
	await shot("annotation-compact");
	await dialog.locator("textarea").fill("Manter seleção completa.");
	await button("Adicionar como evidência").click();
	action = await lastAction();
	assert(
		action.request.snippet.length > 6000,
		"preview truncation never truncates submitted source",
	);
	assert.deepEqual(errors, []);
	console.log(
		"PASS: diff sides and gutter actions, file range, editable selection, submit/new session/composer/review, focus, Escape, bounded previews, themes, compact layout. Screenshots: " +
			output,
	);
} catch (error) {
	console.error(errors);
	await shot("failure");
	throw error;
} finally {
	await browser.close();
}
