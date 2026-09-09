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
const page = await browser.newPage({ viewport: { width: 1150, height: 900 } });
page.setDefaultTimeout(10000);
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
page.on("console", (message) => {
	if (message.type() === "error") errors.push(message.text());
});
const button = (name) => page.getByRole("button", { name, exact: true });
const root = page.locator(".dcc-activity-timeline");
const steps = root.locator(".dcc-activity-step");
const output = process.env.DCC_SCREENSHOT_DIR || "/tmp/dcc-activity-smoke";
const filter = (name) =>
	root
		.getByRole("group", { name: "Filtrar atividades" })
		.getByRole("button", { name: new RegExp(`^${name} `) })
		.click();
try {
	await page.goto(
		`${process.env.DCC_ACTIVITY_URL || "http://127.0.0.1:1432"}/tests/activity.html`,
	);
	await root.waitFor();
	assert.equal(
		await steps.count(),
		0,
		"collapsed history does not mount 1000 entries",
	);
	await root
		.locator(".dcc-activity-latest")
		.getByText("read src/module-999.ts", { exact: false })
		.waitFor();
	await button("Mostrar atividades").focus();
	await page.keyboard.press("Enter");
	assert.equal(await steps.count(), 20);
	assert.equal(
		await root.getByText("RESULT-999", { exact: false }).count(),
		0,
		"closed tool output is not mounted",
	);
	const lastTool = steps.last().locator("summary");
	await lastTool.focus();
	await page.keyboard.press("Enter");
	await root.getByText("RESULT-999", { exact: false }).waitFor();
	await root.locator(".dcc-activity-earlier").click();
	assert.equal(await steps.count(), 40);
	const indexes = await steps.evaluateAll((rows) =>
		rows.map((row) => Number(row.dataset.activityId.split("-")[1])),
	);
	assert.deepEqual(
		indexes,
		Array.from({ length: 40 }, (_, i) => 960 + i),
	);
	await filter("Atualizações");
	await root.getByText("Nenhum registro neste filtro.").waitFor();
	await button("Falha antiga").click();
	await button("Ver 1 falha").click();
	assert.equal(await steps.count(), 1);
	await root.getByText("EXPECTED-FAILURE", { exact: false }).waitFor();
	await button("Recolher atividades").click();
	assert.equal(await steps.count(), 0);
	await button("Ver 1 falha").click();
	await root.getByText("EXPECTED-FAILURE", { exact: false }).waitFor();
	await button("Execução ativa").click();
	await root.getByRole("status").getByText("Agente trabalhando").waitFor();
	await button("Concluir").click();
	await page.waitForFunction(
		() =>
			document.querySelector(".dcc-activity-timeline")?.dataset.state ===
			"closed",
	);
	await button("Execução ativa").click();
	const readTool = root.locator('[data-activity-id="read"] summary');
	await readTool.click();
	await button("Concluir").click();
	await page.waitForTimeout(600);
	assert.equal(
		await root.getAttribute("data-state"),
		"open",
		"manual inspection is preserved after completion",
	);
	await button("Execução ativa").click();
	const liveOutput = root.locator('[data-activity-id="test"] details > div');
	await liveOutput.focus();
	await button("Concluir").click();
	await page.waitForTimeout(600);
	assert.equal(
		await liveOutput.isVisible(),
		true,
		"inspected live output stays open after completion",
	);
	await button("Execução ativa").click();
	await button("Recolher atividades").click();
	await button("Adicionar evento").click();
	assert.equal(
		await root.getAttribute("data-state"),
		"closed",
		"live updates respect manual collapse",
	);
	await root
		.locator(".dcc-activity-latest")
		.getByText("Nova atividade", { exact: false })
		.waitFor();
	await button("Interromper").click();
	assert.equal(await root.getAttribute("data-live"), "false");
	await root.getByText("Execução interrompida", { exact: true }).waitFor();
	await button("Mostrar atividades").click();
	await root.getByText("Sem conclusão registrada").first().waitFor();
	await button("Pedir aprovação").click();
	await root.getByText("Aguardando você").waitFor();
	await filter("Falhas");
	await page.getByText("Executar verificação local", { exact: true }).waitFor();
	await page.getByText("Revisão de testes", { exact: true }).waitFor();
	await button("Histórico longo").click();
	await button("Mostrar atividades").click();
	const oldestVisible = root.locator('[data-activity-id="tool-980"]');
	await oldestVisible.locator("summary").click();
	await button("Adicionar evento").click();
	assert.equal(
		await oldestVisible.locator("details").getAttribute("open"),
		"",
		"new events preserve inspected entries and their disclosure state",
	);
	await button("Execução ativa").click();
	await mkdir(output, { recursive: true });
	await page.screenshot({
		path: `${output}/live-dark.png`,
		animations: "disabled",
		fullPage: true,
	});
	await button("Trocar tema").click();
	await page.screenshot({
		path: `${output}/live-light.png`,
		animations: "disabled",
		fullPage: true,
	});
	await page.setViewportSize({ width: 420, height: 820 });
	await page.emulateMedia({ reducedMotion: "reduce" });
	await page.screenshot({
		path: `${output}/compact.png`,
		animations: "disabled",
		fullPage: true,
	});
	assert.equal(
		await page.evaluate(
			() => document.documentElement.scrollWidth <= innerWidth,
		),
		true,
	);
	assert.deepEqual(await page.evaluate(() => window.activityIpc), []);
	assert.deepEqual(errors, []);
	console.log(
		"PASS: bounded history, lazy output, chronological pagination, filters, failure visibility, live summary, completion/interruption/waiting, manual choices, keyboard, themes and compact layout. Screenshots: " +
			output,
	);
} catch (error) {
	await mkdir(output, { recursive: true });
	await page.screenshot({ path: `${output}/failure.png`, fullPage: true });
	console.error({ errors });
	throw error;
} finally {
	await browser.close();
}
