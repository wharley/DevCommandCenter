import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
const { chromium } = await import(process.env.DCC_PLAYWRIGHT_MODULE || "playwright");
const browser = await chromium.launch({ headless: true, ...(process.env.DCC_CHROMIUM_EXECUTABLE ? { executablePath: process.env.DCC_CHROMIUM_EXECUTABLE } : {}) });
const page = await browser.newPage({ viewport: { width: 368, height: 480 }, deviceScaleFactor: 2 });
const errors = [];
page.on("pageerror", error => errors.push(error.message));
const base = `${process.env.DCC_MENU_BAR_URL || "http://localhost:5173"}/apps/desktop/tests/menu-bar.html`;
const output = process.env.DCC_MENU_BAR_SCREENSHOTS || "/tmp/dcc-menu-bar";
await mkdir(output, { recursive: true });
try {
	for (const theme of ["dark", "light"]) {
		await page.goto(`${base}?theme=${theme}`);
		await page.getByText("2 executando").waitFor();
		assert.equal(await page.getByRole("button", { name: /Aguardando aprovação/ }).count(), 1);
		assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth || document.documentElement.scrollHeight > innerHeight), false);
		await page.screenshot({ path: `${output}/${theme}.png` });
	}
	await page.getByRole("button", { name: /Atualizar autenticação/ }).click();
	assert.deepEqual(JSON.parse(await page.locator("body").getAttribute("data-action")), { command: "menu_bar_open_main", args: { workspaceId: "w0", sessionId: "s0" } });
	await page.getByRole("button", { name: "Nova tarefa" }).click();
	assert.equal(JSON.parse(await page.locator("body").getAttribute("data-action")).command, "menu_bar_compose");
	await page.goto(`${base}?empty&lang=en`);
	await page.getByText("No tasks running", { exact: true }).waitFor();
	await page.screenshot({ path: `${output}/empty.png` });
	await page.goto(`${base}?error`);
	await page.getByRole("alert").waitFor();
	assert.equal(await page.getByText("Nenhuma tarefa executando").count(), 0);
	await page.screenshot({ path: `${output}/error.png` });
	assert.deepEqual(errors, []);
	console.log("Menu bar smoke passed: dark/light, empty/error, task navigation, composer, no overflow.");
} finally { await browser.close(); }
