// Real UI components; isolated fixture callbacks. No native database or agent.
import assert from "node:assert/strict";
import fs from "node:fs/promises";
const { chromium } = await import(
	process.env.DCC_PLAYWRIGHT_MODULE || "playwright"
);
const artifacts = process.env.DCC_DESIGN_ARTIFACTS || "/tmp/dcc-design-smoke";
await fs.mkdir(artifacts, { recursive: true });
const browser = await chromium.launch({
	headless: true,
	...(process.env.DCC_CHROMIUM_EXECUTABLE
		? { executablePath: process.env.DCC_CHROMIUM_EXECUTABLE }
		: {}),
});
const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
const errors = [];
page.setDefaultTimeout(10000);
page.on("pageerror", (error) => errors.push(error.message));
const capture = () =>
	page.locator(".dcc-execution-footer .notes-capture-button");
const screenshot = async (name) => {
	await page.mouse.move(1100, 150);
	await page.waitForTimeout(350);
	await page.screenshot({ path: `${artifacts}/${name}.png` });
};
const checkFooter = async () => {
	const branch = await page.locator(".dcc-execution-branch").boundingBox();
	const button = await capture().boundingBox();
	assert.ok(branch && button);
	assert.ok(branch.width >= 40, "branch retains readable space");
	assert.ok(
		button.x + button.width + 12 <= branch.x,
		"capture sits between the worktree control and the project context",
	);
	assert.ok(
		button.x + button.width <= page.viewportSize().width,
		"capture stays in viewport",
	);
	assert.ok(
		button.y + button.height <= page.viewportSize().height,
		"footer stays in viewport",
	);
	assert.equal(
		await capture().innerText(),
		"",
		"icon only, label lives in tooltip",
	);
	assert.equal(
		await capture().evaluate((el) => getComputedStyle(el).position),
		"relative",
	);
	const branchText = await page
		.locator(".dcc-execution-branch > span")
		.evaluate((el) => ({
			text: el.textContent,
			clipped: el.scrollWidth > el.clientWidth,
		}));
	assert.equal(
		branchText.clipped,
		false,
		"short and long branch names remain fully readable",
	);
	const hit = await page.evaluate(
		({ x, y }) =>
			Boolean(
				document.elementFromPoint(x, y)?.closest(".dcc-execution-branch"),
			),
		{ x: branch.x + branch.width / 2, y: branch.y + branch.height / 2 },
	);
	assert.ok(hit, "branch is not occluded");
};
try {
	await page.goto(
		`${process.env.DCC_NOTES_URL || "http://127.0.0.1:1432"}/tests/notes.html?design`,
	);
	await page.locator(".dcc-project-shortcut").first().waitFor();
	await screenshot("new-task-dark");
	await page.getByRole("button", { name: "Trocar tema" }).click();
	await screenshot("new-task-light");
	await page
		.locator(".dcc-project-shortcut")
		.first()
		.evaluate((button) => {
			button.click();
			button.click();
		});
	await page.locator(".dcc-execution-footer").waitFor();
	assert.equal(await page.evaluate(() => window.designTaskCalls), 1);
	assert.equal(
		await page.evaluate(() => window.designTaskMode),
		"protectedWorktree",
	);
	await page.locator('.dcc-project-heading[data-current="true"]').waitFor();
	await checkFooter();
	await screenshot("workbench-light");
	await page.getByRole("button", { name: "Trocar tema" }).click();
	await screenshot("workbench-dark");
	await page.locator('[data-workspace-id="workspace-notes"]').click();
	assert.equal(
		await page
			.locator('[data-workspace-id="workspace-notes"]')
			.getAttribute("aria-current"),
		"location",
	);
	await page.locator('[data-workspace-id="workspace"]').focus();
	await page.keyboard.press("Enter");
	assert.equal(
		await page
			.locator('[data-workspace-id="workspace"]')
			.getAttribute("aria-current"),
		"location",
	);
	// Nested row actions must receive Enter instead of selecting the parent row.
	await page
		.locator('[data-workspace-id="workspace-notes"]')
		.getByRole("button", { name: "Concluir workspace", exact: true })
		.focus();
	await page.keyboard.press("Enter");
	await page
		.locator('[data-workspace-id="workspace-notes"]')
		.waitFor({ state: "hidden" });
	await page.getByRole("button", { name: /^Concluídos/ }).click();
	await page
		.locator('[data-workspace-id="workspace-notes"][data-status="completed"]')
		.waitFor();
	await page
		.locator('[data-workspace-id="workspace-notes"]')
		.getByRole("button", { name: "Voltar para o projeto" })
		.focus();
	await page.keyboard.press("Enter");
	await page
		.locator('[data-workspace-id="workspace-notes"][data-status="ready"]')
		.waitFor();
	// Returning to the task now restores its notes. Dismiss them before testing a new capture.
	for (const close of await page
		.locator(".note-balloon")
		.getByRole("button", { name: "Fechar balão e manter anotação salva" })
		.all()) {
		await close.click();
	}
	await page.locator(".note-balloon").waitFor({ state: "hidden" });
	await capture().focus();
	await page.getByRole("tooltip").waitFor();
	assert.match(await page.getByRole("tooltip").innerText(), /Anotar/);
	await page.keyboard.press("Escape");
	await page.locator("#capture-text").evaluate((element) => {
		const range = document.createRange();
		range.selectNodeContents(element);
		const selection = window.getSelection();
		selection.removeAllRanges();
		selection.addRange(range);
	});
	await capture().click();
	await page.getByLabel("Texto da anotação").waitFor();
	await page.getByRole("button", { name: "Contexto preservado" }).click();
	assert.ok(
		await page
			.locator(".note-balloon")
			.getByText(/Ao implementar o login/)
			.count(),
	);
	await screenshot("captured-note-dark");
	await page
		.locator(".note-balloon")
		.getByRole("button", { name: /Fechar/ })
		.click();
	for (const width of [1000, 800]) {
		await page.setViewportSize({ width, height: 900 });
		await checkFooter();
		await page.getByRole("button", { name: "Trocar branch" }).click();
		await checkFooter();
		await page.getByRole("button", { name: "Trocar branch" }).click();
	}
	await page.locator(".note-balloon").waitFor({ state: "hidden" });
	await page
		.getByRole("button", {
			name: /Recolher.*barra|Recolher.*lateral|Recolher sidebar/i,
		})
		.click();
	await page.setViewportSize({ width: 640, height: 900 });
	await checkFooter();
	await screenshot("compact-footer");
	await page
		.getByRole("button", {
			name: /Expandir.*barra|Expandir.*lateral|Expandir sidebar/i,
		})
		.click();
	await page.setViewportSize({ width: 1440, height: 960 });
	await page.getByRole("button", { name: /Abrir configurações/i }).click();
	await page.getByRole("dialog").waitFor();
	await screenshot("dialog-dark");
	await page.keyboard.press("Escape");
	await page.emulateMedia({ reducedMotion: "reduce" });
	await page.locator(".dcc-sidebar-navigation > button").first().click();
	await page.locator(".dcc-task-launch-content").waitFor();
	assert.equal(
		await page
			.locator(".dcc-task-launch-content")
			.evaluate((el) => getComputedStyle(el).animationName),
		"none",
	);
	assert.deepEqual(errors, []);
	console.log(
		"PASS: themes, real sidebar selection and keyboard navigation, project creation guard, icon capture with context, branch hit testing at multiple widths, collapsed sidebar, modal and reduced motion.",
	);
	console.log(`Screenshots: ${artifacts}`);
} finally {
	await browser.close();
}
