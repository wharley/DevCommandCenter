// Run only against feedback.html: all publication and authentication are synthetic.
import assert from "node:assert/strict";
import fs from "node:fs/promises";
const { chromium } = await import(
	process.env.DCC_PLAYWRIGHT_MODULE || "playwright"
);
const artifacts =
	process.env.DCC_FEEDBACK_ARTIFACTS || "/tmp/dcc-feedback-smoke";
await fs.mkdir(artifacts, { recursive: true });
const browser = await chromium.launch({
	headless: true,
	...(process.env.DCC_CHROMIUM_EXECUTABLE
		? { executablePath: process.env.DCC_CHROMIUM_EXECUTABLE }
		: {}),
});
const page = await browser.newPage({ viewport: { width: 1360, height: 900 } });
page.setDefaultTimeout(10_000);
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
const url = `${process.env.DCC_FEEDBACK_URL || "http://127.0.0.1:1442"}/tests/feedback.html`;
const open = async () => {
	await page.getByRole("button", { name: "Feedback DCC", exact: true }).click();
	await page.getByText("Enviando como @alice").waitFor();
};
const fixture = async () => page.evaluate(() => window.feedbackFixture);
try {
	await page.goto(url);
	await open();
	assert.equal(
		await page
			.getByRole("button", { name: "Revisar feedback", exact: true })
			.isDisabled(),
		true,
	);
	await page
		.getByLabel("Título", { exact: true })
		.fill("A barra lateral não reabre");
	await page
		.getByLabel("Descrição", { exact: true })
		.fill(
			"Ao recolher a barra lateral e tentar abrir novamente, o botão não responde.",
		);
	await page.screenshot({ animations: "disabled", path: `${artifacts}/new-dark.png` });
	await page.keyboard.press("Escape");
	await page.reload();
	await open();
	assert.equal(
		await page.getByLabel("Título", { exact: true }).inputValue(),
		"A barra lateral não reabre",
	);
	await page
		.getByRole("button", { name: "Revisar feedback", exact: true })
		.click();
	assert.equal((await fixture()).createCalls.length, 0);
	await page.getByRole("button", { name: "Editar", exact: true }).click();
	await page.getByLabel("Incluir versão e sistema").uncheck();
	await page
		.getByRole("button", { name: "Revisar feedback", exact: true })
		.click();
	await page.screenshot({ animations: "disabled", path: `${artifacts}/review-dark.png` });
	await page
		.getByRole("button", { name: "Publicar issue", exact: true })
		.click();
	await page.getByText("Issue #43 criada!").waitFor();
	assert.equal((await fixture()).createCalls.length, 1);
	assert.equal((await fixture()).createCalls[0].includeDiagnostics, false);
	assert.equal((await fixture()).createCalls[0].login, "alice");
	await page
		.getByText("Encerrado sem implementação", { exact: true })
		.waitFor();
	await page.getByText("Concluído", { exact: true }).waitFor();
	await page.screenshot({ animations: "disabled", path: `${artifacts}/history-dark.png` });
	await page
		.getByRole("button", { name: /\[Bug\]: A barra lateral não reabre/ })
		.first()
		.click();
	assert.match(
		(await fixture()).external.at(-1),
		/^https:\/\/github.com\/wharley\/DevCommandCenter\/issues\/\d+$/,
	);
	await page.evaluate(() => {
		window.feedbackFixture.offline = true;
	});
	await page.getByRole("button", { name: "Atualizar feedbacks" }).click();
	await page.getByText(/Exibindo a última consulta salva/).waitFor();
	assert.ok(
		await page
			.getByText("Encerrado sem implementação", { exact: true })
			.isVisible(),
	);
	await page.keyboard.press("Escape");
	await page.getByRole("button", { name: "Trocar tema", exact: true }).click();
	await open();
	await page.screenshot({ animations: "disabled", path: `${artifacts}/history-light.png` });
	await page.keyboard.press("Escape");
	await page
		.getByRole("button", { name: "Recolher barra lateral", exact: true })
		.click();
	await open();
	await page.getByRole("tab", { name: "Novo feedback", exact: true }).click();
	await page.getByRole("button", { name: "Melhoria", exact: true }).click();
	await page
		.getByLabel("Título", { exact: true })
		.fill("Preservar meu rascunho");
	await page
		.getByLabel("Descrição", { exact: true })
		.fill("Gostaria de retomar meu relato após reiniciar o DCC.");
	await page.screenshot({ animations: "disabled", path: `${artifacts}/new-light.png` });
	await page.evaluate(() => {
		window.feedbackFixture.uncertain = true;
	});
	await page
		.getByRole("button", { name: "Revisar feedback", exact: true })
		.click();
	await page
		.getByRole("button", { name: "Publicar issue", exact: true })
		.click();
	await page.getByText(/O GitHub ainda não confirmou/).waitFor();
	const uncertainInput = (await fixture()).createCalls.at(-1);
	await page.keyboard.press("Escape");
	await page.reload();
	await open();
	await page
		.getByRole("button", { name: "Verificar envio", exact: true })
		.click();
	await page.getByText("Issue #43 criada!").waitFor();
	assert.deepEqual((await fixture()).createCalls[0], uncertainInput);
	await page.keyboard.press("Escape");
	await page.evaluate(() => {
		window.feedbackFixture.login = "bob";
	});
	await page.getByRole("button", { name: "Feedback DCC", exact: true }).click();
	await page.getByText("Enviando como @bob").waitFor();
	await page.getByText("Sua experiência pode fazer a diferença.").waitFor();
	assert.equal(
		await page
			.getByText("Encerrado sem implementação", { exact: true })
			.count(),
		0,
	);
	await page.keyboard.press("Escape");
	await page.getByRole("button", { name: "Idioma", exact: true }).click();
	await page.getByRole("button", { name: "DCC Feedback", exact: true }).click();
	await page.getByRole("tab", { name: "New feedback", exact: true }).click();
	await page
		.getByRole("button", { name: "Improvement", exact: true })
		.waitFor();
	await page.setViewportSize({ width: 440, height: 760 });
	await page.screenshot({ animations: "disabled", path: `${artifacts}/compact-english.png` });
	assert.equal(
		await page.evaluate(
			() => document.documentElement.scrollWidth > innerWidth,
		),
		false,
	);
	await page.keyboard.press("Escape");
	await page.evaluate(() => {
		window.feedbackFixture.disconnected = true;
	});
	await page.getByRole("button", { name: "DCC Feedback", exact: true }).click();
	await page.getByText(/Connect your GitHub account in Settings/).waitFor();
	await page
		.getByRole("button", { name: "Open Settings", exact: true })
		.click();
	assert.equal((await fixture()).settings, 1);
	assert.deepEqual(errors, []);
	console.log(
		"Feedback smoke passed: sidebar modes, drafts, review, submission, history/status, offline cache, uncertain recovery, account isolation, themes, English, compact layout, disconnected account.",
	);
} finally {
	await browser.close();
}
