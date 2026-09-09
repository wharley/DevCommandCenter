// Exercises the real settings dialog with synthetic IPC; never touches live accounts.
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
const page = await browser.newPage({ viewport: { width: 1280, height: 930 } });
page.setDefaultTimeout(10000);
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
page.on("console", (message) => {
	if (message.type() === "error") errors.push(message.text());
});
const button = (name) => page.getByRole("button", { name, exact: true });
const dialog = page.getByRole("dialog", { name: "Configurações", exact: true });
const nav = page.getByRole("navigation", { name: "Seções das configurações" });
const section = async (name) => {
	await nav.getByRole("button", { name, exact: true }).click();
	await page.getByRole("heading", { name, exact: true, level: 2 }).waitFor();
};
const output = process.env.DCC_SCREENSHOT_DIR || "/tmp/dcc-settings-smoke";
try {
	await page.goto(
		`${process.env.DCC_SETTINGS_URL || "http://127.0.0.1:1432"}/tests/settings.html`,
	);
	await button("Abrir configurações").click();
	await dialog.waitFor();
	assert.equal(await nav.getByRole("button").count(), 9);
	await button("Verificar atualizações").click();
	assert.equal(await page.evaluate(() => window.settingsUpdateChecks), 1);
	const search = page.getByRole("textbox", { name: "Buscar seções…" });
	await search.fill("atualizacao");
	assert.equal(
		await nav.getByRole("button").count(),
		1,
		"search ignores diacritics",
	);
	await search.fill("idioma");
	await search.press("Enter");
	await page
		.getByRole("heading", { name: "Aparência", exact: true, level: 2 })
		.waitFor();
	assert.equal(
		await nav
			.getByRole("button", { name: "Aparência" })
			.getAttribute("aria-current"),
		"page",
	);
	await search.fill("no matching section");
	await page.getByRole("status").getByText("Nenhuma seção encontrada").waitFor();
	await button("Mostrar todas as seções").click();
	assert.equal(await nav.getByRole("button").count(), 9);
	await search.fill("MCP");
	await search.press("Escape");
	assert.equal(await search.inputValue(), "");
	assert.equal(
		await dialog.isVisible(),
		true,
		"Escape clears search before dismissing dialog",
	);
	assert.equal(await dialog.getAttribute("data-state"), "open");
	await mkdir(output, { recursive: true });
	await page.screenshot({ animations: "disabled", path: `${output}/appearance-dark.png` });
	await button("Claro").click();
	assert.equal(
		await page.evaluate(() => localStorage.getItem("dcc-theme")),
		"light",
	);
	assert.equal(await button("Claro").getAttribute("aria-pressed"), "true");
	await page.screenshot({ animations: "disabled", path: `${output}/appearance-light.png` });
	await page.getByRole("radio", { name: "Compacta", exact: true }).click();
	assert.equal(
		await page.evaluate(() => localStorage.getItem("dcc-density")),
		"compact",
	);
	await page.getByRole("radio", { name: "English", exact: true }).click();
	await page
		.getByRole("heading", { name: "Appearance", exact: true, level: 2 })
		.waitFor();
	await page.getByRole("textbox", { name: "Find a section…" }).fill("language");
	assert.equal(
		await page
			.getByRole("navigation", { name: "Settings sections" })
			.getByRole("button")
			.count(),
		1,
	);
	await button("Clear section search").click();
	await page.getByRole("radio", { name: "Portuguese (Brazil)", exact: true }).click();
	for (const name of [
		"Provedores",
		"Integrações",
		"Conta",
		"Conexões",
		"Git",
		"Experimental",
		"Atalhos",
	])
		await section(name);
	await button("Abrir atalhos").click();
	assert.equal(await page.evaluate(() => window.settingsShortcuts), 1);
	await section("Git");
	assert.equal(
		await page.locator(".dcc-settings-content button").first().isDisabled(),
		true,
		"workspace actions still require a workspace",
	);
	await section("Aparência");
	await page.setViewportSize({ width: 420, height: 740 });
	await page.emulateMedia({ reducedMotion: "reduce" });
	const select = page.getByRole("combobox", { name: "Seção", exact: true });
	await select.waitFor();
	assert.equal(await select.locator("option").count(), 9);
	await page.screenshot({ animations: "disabled", path: `${output}/compact.png` });
	for (const id of [
		"general",
		"appearance",
		"model",
		"integrations",
		"account",
		"connections",
		"git",
		"experimental",
		"shortcuts",
	]) {
		await select.selectOption(id);
		await page.waitForTimeout(150);
		const geometry = await page
			.locator(".dcc-settings-content")
			.evaluate((el) => ({
				width: el.clientWidth,
				scrollWidth: el.scrollWidth,
			}));
		assert.ok(
			geometry.scrollWidth <= geometry.width + 1,
			`no horizontal overflow in ${id}: ${JSON.stringify(geometry)}`,
		);
	}
	await select.selectOption("appearance");
	await button("Escuro").click();
	await page.keyboard.press("Escape");
	await dialog.waitFor({ state: "hidden" });
	assert.equal(
		await button("Abrir configurações").evaluate(
			(el) => el === document.activeElement,
		),
		true,
		"closing restores trigger focus",
	);
	assert.deepEqual(await page.evaluate(() => window.unexpectedSettingsCalls), []);
	await page.reload();
	await button("Abrir configurações").click();
	await select.selectOption("appearance");
	assert.equal(await button("Escuro").getAttribute("aria-pressed"), "true");
	assert.equal(
		await page.evaluate(() => localStorage.getItem("dcc-density")),
		"compact",
	);
	assert.deepEqual(
		await page.evaluate(() => window.unexpectedSettingsCalls),
		[],
	);
	assert.deepEqual(errors, []);
	console.log(
		"PASS: all nine sections, search, keyboard, update/shortcut callbacks, theme/density/locale persistence, compact layout and reduced-motion dialog lifecycle. Screenshots: " +
			output,
	);
} catch (error) {
  await mkdir(output, { recursive: true });
  await page.screenshot({ animations: "disabled", path: `${output}/failure.png` });
  console.error({ errors, ipc: await page.evaluate(() => window.unexpectedSettingsCalls) });
  throw error;
} finally {
	await browser.close();
}
