// Run against the isolated Vite fixture. Never opens a real DCC database or provider.
import assert from "node:assert/strict";
import fs from "node:fs/promises";
const { chromium } = await import(
	process.env.DCC_PLAYWRIGHT_MODULE || "playwright"
);
const artifacts = process.env.DCC_NOTES_ARTIFACTS || "/tmp/dcc-notes-smoke";
await fs.mkdir(artifacts, { recursive: true });
const browser = await chromium.launch({
	headless: true,
	...(process.env.DCC_CHROMIUM_EXECUTABLE
		? { executablePath: process.env.DCC_CHROMIUM_EXECUTABLE }
		: {}),
});
const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
page.setDefaultTimeout(10000);
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
const saved = () =>
	page.evaluate(() =>
		JSON.parse(localStorage.getItem("fixture.notes") || "[]"),
	);
const waitSaved = async () => {
	await page.getByText("Salvo no projeto", { exact: true }).waitFor();
};
const library = async () => {
	await page.getByRole("button", { name: "Anotações", exact: true }).click();
	await page
		.getByRole("heading", { name: "Espaço para a próxima ideia." })
		.waitFor();
};
try {
	await page.goto(
		`${process.env.DCC_NOTES_URL || "http://127.0.0.1:1432"}/tests/notes.html`,
	);
	await page
		.getByRole("heading", { name: "Espaço para a próxima ideia." })
		.waitFor();
	await page.getByLabel("Filtrar por projeto").selectOption("all");
	await page.waitForTimeout(450);
	await page.screenshot({ path: `${artifacts}/library-dark.png` });
	await page.keyboard.press("Escape");
	await page.getByRole("button", { name: "Trocar tema" }).click();
	await library();
	await page.getByLabel("Filtrar por projeto").selectOption("all");
	await page.waitForTimeout(450);
	await page.screenshot({ path: `${artifacts}/library-light.png` });
	await page
		.getByRole("button")
		.filter({
			has: page.getByRole("heading", { name: "Sessões sob controle" }),
		})
		.click();
	await page
		.getByLabel("Texto da anotação")
		.fill("Listar dispositivos e revogar uma sessão por vez.");
	await waitSaved();
	assert.equal(
		(await saved()).find((note) => note.id === "note-login").content,
		"Listar dispositivos e revogar uma sessão por vez.",
	);
	await page.getByRole("button", { name: "Contexto preservado" }).click();
	const handle = page.getByRole("button", {
		name: "Arraste para mover · setas para ajustar a posição",
	});
	const rect = await handle.boundingBox();
	await page.mouse.move(rect.x + 25, rect.y + 12);
	await page.mouse.down();
	await page.mouse.move(600, 210, { steps: 15 });
	await page.mouse.up();
	await page.waitForTimeout(450);
	await page.screenshot({ path: `${artifacts}/balloon-light.png` });
	await handle.focus();
	await page.keyboard.press("ArrowLeft");
	await page.getByRole("button", { name: "Recolher para uma bolha" }).click();
	await page.getByRole("button", { name: "Expandir anotação" }).click();
	await page
		.getByRole("button", { name: "Manter visível em outros projetos" })
		.click();
	await waitSaved();
	await page.getByRole("button", { name: "Trocar projeto" }).click();
	assert.equal(await page.getByLabel("Texto da anotação").count(), 1);
	await page
		.getByRole("button", { name: "Mostrar apenas neste projeto" })
		.click();
	await waitSaved();
	await page.getByRole("button", { name: "Trocar projeto" }).click();
	await page.getByLabel("Texto da anotação").waitFor();
	await page
		.getByRole("button", { name: "Fechar balão e manter anotação salva" })
		.click();
	await page.getByLabel("Texto da anotação").waitFor({ state: "hidden" });
	await library();
	await page.getByLabel("Filtrar por projeto").selectOption("other");
	await page
		.getByRole("heading", { name: "Uma busca que entende o contexto" })
		.click();
	await page.getByLabel("Texto da anotação").waitFor();
	assert.equal(
		await page
			.getByRole("button", { name: "Adicionar à conversa atual" })
			.isDisabled(),
		true,
		"a note from another project cannot enter the current composer",
	);
	await page
		.getByText(
			"Para adicionar à conversa, selecione uma tarefa do projeto Orbit ou crie uma tarefa a partir desta anotação.",
			{ exact: true },
		)
		.waitFor();
	assert.equal(await page.getByLabel("Composer de teste").inputValue(), "");
	await page.getByRole("button", { name: "Trocar projeto" }).click();
	await library();
	await page
		.getByRole("heading", { name: "Uma busca que entende o contexto" })
		.click();
	await page
		.getByRole("button", { name: "Adicionar à conversa atual" })
		.click();
	assert.match(
		await page.getByLabel("Composer de teste").inputValue(),
		/Explorar filtros/,
	);
	await page.getByLabel("Texto da anotação").waitFor({ state: "hidden" });
	await page.getByRole("button", { name: "Trocar projeto" }).click();
	// Switching back restores this task's source note; dismiss it before capture.
	await page
		.locator('[data-note-id="note-login"]')
		.getByRole("button", { name: "Fechar balão e manter anotação salva" })
		.click();
	await page.getByLabel("Texto da anotação").waitFor({ state: "hidden" });
	await page.evaluate(() => {
		const node = document.querySelector("#capture-text");
		const range = document.createRange();
		range.selectNodeContents(node);
		const selection = window.getSelection();
		selection.removeAllRanges();
		selection.addRange(range);
	});
	await page.keyboard.press("Meta+Shift+N");
	await page.getByLabel("Título da anotação").fill("Uma ideia capturada");
	await page
		.getByLabel("Texto da anotação")
		.fill("Acompanhar a implementação depois.");
	await waitSaved();
	const captured = (await saved()).find(
		(note) => note.title === "Uma ideia capturada",
	);
	assert.match(captured.contextSnapshot, /dispositivos conectados/);
	await page.evaluate(() => {
		window.notesFailSave = true;
	});
	await page
		.getByLabel("Texto da anotação")
		.fill("Texto protegido durante falha de gravação.");
	await page
		.getByRole("button", { name: "Não foi salvo. Tentar novamente" })
		.waitFor();
	assert.equal(
		await page.getByLabel("Texto da anotação").inputValue(),
		"Texto protegido durante falha de gravação.",
	);
	await page.evaluate(() => {
		window.notesFailSave = false;
	});
	await page
		.getByRole("button", { name: "Não foi salvo. Tentar novamente" })
		.click();
	await waitSaved();
	await page.getByRole("button", { name: "Criar tarefa", exact: true }).click();
	await waitSaved();
	await page
		.getByRole("button", { name: "Fechar balão e manter anotação salva" })
		.click();
	await page.getByLabel("Texto da anotação").waitFor({ state: "hidden" });
	assert.equal(await page.evaluate(() => window.notesTaskCalls), 1);
	assert.equal(
		(await saved()).find((note) => note.id === captured.id)
			.implementationTaskId,
		"task-from-note",
	);
	await page.getByRole("button", { name: "Concluir tarefa criada" }).click();
	await page.getByRole("button", { name: "Manter abertas" }).click();
	assert.equal(
		(await saved()).find((note) => note.id === captured.id).status,
		"open",
	);
	await page.getByRole("button", { name: "Concluir tarefa criada" }).click();
	await page.getByRole("button", { name: "Marcar como concluída" }).click();
	await page
		.getByRole("heading", { name: "Essa ideia já foi implementada?" })
		.waitFor({ state: "hidden" });
	assert.equal(
		(await saved()).find((note) => note.id === captured.id).status,
		"completed",
	);
	await library();
	await page.getByRole("button", { name: /^Concluídas/ }).click();
	await page.getByRole("heading", { name: "Uma ideia capturada" }).click();
	await page.getByRole("button", { name: "Reabrir anotação" }).click();
	await page.getByLabel("Texto da anotação").waitFor({ state: "hidden" });
	assert.equal(
		(await saved()).find((note) => note.id === captured.id).status,
		"open",
	);
	await page.reload();
	await page.getByRole("heading", { name: "Uma ideia capturada" }).click();
	const restoredSource = page.locator('[data-note-id="note-login"]');
	await restoredSource
		.getByRole("button", { name: "Fechar balão e manter anotação salva" })
		.click();
	await restoredSource.waitFor({ state: "hidden" });
	assert.equal(
		await page.getByLabel("Texto da anotação").inputValue(),
		"Texto protegido durante falha de gravação.",
	);
	await page.getByRole("button", { name: "Marcar como concluída" }).click();
	await page.getByLabel("Texto da anotação").waitFor({ state: "hidden" });
	await library();
	await page.getByRole("button", { name: /^Concluídas/ }).click();
	await page
		.getByRole("button", { name: "Excluir concluídas deste filtro" })
		.click();
	await page.getByRole("button", { name: "Excluir", exact: true }).click();
	await page
		.getByRole("heading", { name: "Excluir esta anotação?" })
		.waitFor({ state: "hidden" });
	assert.equal((await saved()).length, 3);
	await page.getByRole("button", { name: /^Abertas/ }).click();
	await page.getByRole("heading", { name: "Sessões sob controle" }).click();
	await page.setViewportSize({ width: 1000, height: 700 });
	const balloon = await page.locator(".note-balloon").boundingBox();
	assert.ok(
		balloon.x >= 0 &&
			balloon.y >= 0 &&
			balloon.x + balloon.width <= 1000 &&
			balloon.y + balloon.height <= 700,
	);
	await page.evaluate(() => document.documentElement.classList.add("dark"));
	await page.waitForTimeout(450);
	await page.screenshot({ path: `${artifacts}/balloon-dark.png` });
	assert.deepEqual(errors, []);
	console.log(
		"PASS: library filters, light/dark, drag, keyboard movement, minimize, cross-project pinning, composer draft, context capture, save recovery, task link, explicit completion, reopen, reload, bulk deletion and viewport bounds.",
	);
	console.log(`Screenshots: ${artifacts}`);
} finally {
	await browser.close();
}
