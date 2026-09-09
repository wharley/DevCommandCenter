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
const page = await browser.newPage({ viewport: { width: 1100, height: 900 } });
page.setDefaultTimeout(12000);
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
page.on("console", (message) => {
	if (message.type() === "error") errors.push(message.text());
});
const button = (name) => page.getByRole("button", { name, exact: true });
const dialog = page.getByRole("dialog", { name: "Skills", exact: true });
const output = process.env.DCC_SCREENSHOT_DIR || "/tmp/dcc-skills-smoke";
const shot = async (name) => {
	await dialog.evaluate((el) =>
		Promise.allSettled(
			el.getAnimations().map((animation) => animation.finished),
		),
	);
	return page.screenshot({
		path: `${output}/${name}.png`,
		animations: "disabled",
		style: "[data-sonner-toaster] { visibility: hidden; }",
	});
};
const cards = page.locator(".skills-card");
const open = async (scenario = "") => {
	await page.goto(
		`${process.env.DCC_SKILLS_URL || "http://127.0.0.1:1432"}/tests/skills.html${scenario ? `?case=${scenario}` : ""}`,
	);
	await button("Abrir skills").click();
	await dialog.waitFor();
};
const withinViewport = async () => {
	// Resizing can overlap the dialog's entrance animation and layout update.
	await page.waitForFunction(() => {
		const box = document
			.querySelector(".skills-dialog")
			?.getBoundingClientRect();
		return (
			box &&
			box.x >= 0 &&
			box.y >= 0 &&
			box.right <= innerWidth &&
			box.bottom <= innerHeight
		);
	});
	const box = await dialog.boundingBox();
	const v = page.viewportSize();
	assert(
		box.x >= 0 &&
			box.y >= 0 &&
			box.x + box.width <= v.width &&
			box.y + box.height <= v.height,
	);
	assert.equal(
		await dialog.evaluate((el) => el.scrollWidth <= el.clientWidth),
		true,
	);
};
try {
	await mkdir(output, { recursive: true });
	await open();
	await cards.first().waitFor();
	assert.equal(await cards.count(), 3);
	await shot("library-light");
	await page.locator(".skills-search input").fill("codex");
	assert.equal(await cards.count(), 1);
	await page.locator(".skills-search input").fill("inexistente");
	await page
		.getByText("Nenhuma skill corresponde à busca.", { exact: true })
		.waitFor();
	await button("Limpar busca").click();
	await page.getByRole("button", { name: /^Detectado no projeto/ }).click();
	await page.locator(".skills-detection").first().waitFor();
	assert.equal(await page.locator(".skills-detection").count(), 3);
	await shot("detected-light");
	await page.getByRole("button", { name: /^Suas skills/ }).click();
	await page.getByRole("button", { name: /^Catálogo DCC/ }).click();
	await shot("catalog-light");
	await button("Usar modelo").click();
	assert.equal(
		await page.locator("#skill-name").inputValue(),
		"dcc-orchestration",
	);
	assert((await page.locator("#skill-body").inputValue()).length > 100);
	await button("Cancelar").click();
	await page.waitForFunction(() =>
		document.activeElement?.textContent?.includes("Catálogo DCC"),
	);
	assert.equal(
		await page.evaluate(() =>
			window.skillCalls.some((call) => call.command === "skills_save"),
		),
		false,
	);
	await page.getByRole("button", { name: /^Suas skills/ }).click();
	assert.equal(await page.locator(".skills-preset").count(), 0);
	await button("Editar review-pr").click();
	assert.equal(await page.locator("#skill-name").isDisabled(), true);
	await page.locator("#skill-desc").fill("Revisar as alterações do projeto.");
	await page
		.locator("#skill-body")
		.fill("# Revisão\n\nPreservar o contexto e testar as mudanças.");
	await button("Claude").click();
	await button("Codex").click();
	await button("Salvar skill").click();
	await page
		.getByText("Escolha pelo menos um agente alvo.", { exact: true })
		.waitFor();
	assert.equal(
		await page.evaluate(
			() =>
				window.skillCalls.filter((call) => call.command === "skills_save")
					.length,
		),
		0,
	);
	for (const agent of ["Claude", "Codex", "Gemini", "Cursor", "Droid · legacy"])
		await button(agent).click();
	await page
		.getByRole("switch", { name: "Desativar invocação pelo modelo" })
		.click();
	await shot("editor-targets-light");
	await button("Salvar skill").click();
	await cards.first().waitFor();
	const saved = await page.evaluate(
		() => window.skillCalls.find((call) => call.command === "skills_save").args,
	);
	assert.equal(saved.workspaceId, "workspace-demo");
	assert.equal(saved.projectRoot, "/fixture/studio");
	assert.equal(saved.skill.targetAgents.length, 5);
	assert.equal(saved.skill.disableModelInvocation, true);
	assert.equal(
		saved.skill.body,
		"# Revisão\n\nPreservar o contexto e testar as mudanças.",
	);
	assert.equal(await page.evaluate(() => window.skillsChanged), 1);
	await button("Nova skill").click();
	await page.locator("#skill-name").fill("Bad Name");
	await button("Salvar skill").click();
	await page.getByText(/O nome deve ter letras minúsculas/).waitFor();
	await page.locator("#skill-name").fill("new-review");
	await page.locator("#skill-desc").fill("Revisar a mudança.");
	await page.locator("#skill-body").fill("Leia o diff.");
	await button("Salvar skill").click();
	await button("Excluir new-review").waitFor();
	await button("Excluir new-review").click();
	await page.waitForFunction(() => window.skillsChanged === 3);
	assert.equal(await cards.count(), 3);
	await page.keyboard.press("Escape");
	await dialog.waitFor({ state: "hidden" });
	await button("Tema").click();
	await button("Abrir skills").click();
	await cards.first().waitFor();
	await shot("library-dark");
	await page.setViewportSize({ width: 390, height: 740 });
	await page.emulateMedia({ reducedMotion: "reduce" });
	await withinViewport();
	await shot("library-compact");
	await page.getByRole("button", { name: /^Catálogo DCC/ }).click();
	await withinViewport();
	await shot("catalog-compact");
	await page.getByRole("button", { name: /^Suas skills/ }).click();
	await button("Editar review-pr").click();
	await page.locator("#skill-desc").waitFor();
	await withinViewport();
	await shot("editor-compact");
	await button("Codex").scrollIntoViewIfNeeded();
	await withinViewport();
	assert.equal(
		await button("Codex").evaluate((el) => el.scrollWidth <= el.clientWidth),
		true,
	);
	await shot("targets-compact");
	await button("Cancelar").click();
	await page.waitForFunction(() =>
		document.activeElement?.matches(".skills-search input"),
	);
	await open("error");
	await page.getByRole("alert").waitFor();
	await button("Tentar novamente").click();
	await cards.first().waitFor();
	await open("empty");
	await page.getByText(/Ainda não há skills/).waitFor();
	await page.getByRole("button", { name: /^Detectado no projeto/ }).click();
	await page.getByText(/Nenhum arquivo de agente/).waitFor();
	await open("readonly");
	await cards.first().waitFor();
	assert.equal(await button("Nova skill").isDisabled(), true);
	assert.equal(await button("Editar review-pr").isDisabled(), true);
	await open("none");
	await page
		.getByText("Selecione um projeto para gerenciar as skills.", {
			exact: true,
		})
		.waitFor();
	assert.equal(await page.evaluate(() => window.skillCalls.length), 0);
	await open("compile-fail");
	await cards.first().waitFor();
	await button("Editar review-pr").click();
	await button("Salvar skill").click();
	await page
		.getByText("Falha de compilação de demonstração.", { exact: true })
		.waitFor();
	assert.equal(await page.locator("#skill-body").isVisible(), true);
	assert.equal(await page.evaluate(() => window.skillsChanged), 0);
	await open("empty");
	await page.getByText(/Ainda não há skills/).waitFor();
	await page.getByRole("button", { name: /^Catálogo DCC/ }).click();
	await button("Usar modelo").click();
	const originalPresetBody = await page.locator("#skill-body").inputValue();
	assert.equal(await page.locator("#skill-name").isDisabled(), true);
	await page
		.locator("#skill-body")
		.fill("Instruções personalizadas do projeto.");
	await button("Adicionar ao projeto").click();
	await button("Editar dcc-orchestration").waitFor();
	assert.equal(await cards.count(), 1);
	await page.getByRole("button", { name: /^Catálogo DCC/ }).click();
	assert.equal(await button("Adicionado").isDisabled(), true);
	await page.getByRole("button", { name: /^Suas skills/ }).click();
	await button("Editar dcc-orchestration").click();
	assert.equal(
		await page.locator("#skill-body").inputValue(),
		"Instruções personalizadas do projeto.",
	);
	await button("Cancelar").click();
	await button("Excluir dcc-orchestration").click();
	await page.waitForFunction(() => window.skillsChanged === 2);
	await page.getByRole("button", { name: /^Catálogo DCC/ }).click();
	await button("Usar modelo").click();
	assert.equal(
		await page.locator("#skill-body").inputValue(),
		originalPresetBody,
	);
	await button("Cancelar").click();
	await open("readonly");
	await cards.first().waitFor();
	await page.getByRole("button", { name: /^Catálogo DCC/ }).click();
	assert.equal(await button("Usar modelo").isDisabled(), true);
	assert.deepEqual(errors, []);
	console.log(
		"PASS: skills search, inventory, preset, edit/create/delete, targets, invocation, validation, errors/retry, read-only/no-project, themes, keyboard, compact layout. Screenshots: " +
			output,
	);
} catch (error) {
	console.error(errors);
	console.error(await page.locator("body").innerText());
	await shot("failure");
	throw error;
} finally {
	await browser.close();
}
