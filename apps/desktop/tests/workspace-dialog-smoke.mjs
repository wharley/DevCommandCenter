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
page.on("pageerror", (e) => errors.push(e.message));
const button = (name) => page.getByRole("button", { name, exact: true });
const dialog = page.getByRole("dialog");
const output =
	process.env.DCC_SCREENSHOT_DIR || "/tmp/dcc-workspace-dialog-smoke";
const open = async (scenario = "") => {
	await page.goto(
		`${process.env.DCC_WORKSPACE_DIALOG_URL || "http://127.0.0.1:1432"}/tests/workspace-dialog.html?case=${scenario}`,
	);
	await button("Abrir modal").click();
	await dialog.waitFor();
};
const fits = async () => {
	await page.waitForFunction(() => {
		const e = document.querySelector(".workspace-dialog");
		if (!e) return false;
		const r = e.getBoundingClientRect();
		return (
			r.x >= 0 &&
			r.y >= 0 &&
			r.right <= innerWidth &&
			r.bottom <= innerHeight &&
			e.scrollWidth <= e.clientWidth
		);
	});
	const d = await dialog.boundingBox();
	const f = await page.locator(".workspace-dialog-footer").boundingBox();
	assert(f.y >= d.y && f.y + f.height <= d.y + d.height + 1);
};
const shot = async (name) => {
	await dialog.evaluate((e) =>
		Promise.allSettled(e.getAnimations().map((a) => a.finished)),
	);
	await page.screenshot({
		path: `${output}/${name}.png`,
		animations: "disabled",
		style: "[data-sonner-toaster] { visibility:hidden; }",
	});
};
const lastCall = async (command) =>
	page.evaluate(
		(command) =>
			window.workspaceCalls.filter((c) => c.command === command).at(-1),
		command,
	);
try {
	await mkdir(output, { recursive: true });
	await open();
	await page.waitForFunction(() =>
		document.querySelector("#workspace-branch")?.value?.startsWith("feature/"),
	);
	await fits();
	await shot("open-light");
	await page.locator("#workspace-name").fill("Revisar projeto");
	await button("Criar tarefa").click();
	await button("Criando…").waitFor();
	assert.equal(await button("Close").count(), 0);
	await page.keyboard.press("Escape");
	assert(await dialog.isVisible());
	await dialog.waitFor({ state: "hidden" });
	assert.equal((await lastCall("create")).args.projectId, "project-0");
	assert.equal((await lastCall("create")).args.isolationMode, null);
	await open();
	await page.getByRole("button", { name: /Vários projetos/ }).click();
	assert(await button("Criar tarefa multi-projeto").isDisabled());
	await page.getByRole("checkbox").nth(0).focus();
	await page.keyboard.press("Space");
	await page.locator(".workspace-dialog-project-option").nth(1).click();
	assert(await page.getByRole("checkbox").nth(0).isChecked());
	assert(await page.getByRole("checkbox").nth(1).isChecked());
	await page.locator("#workspace-name").fill("Integrar projetos");
	await shot("multi-light");
	await button("Criar tarefa multi-projeto").click();
	await dialog.waitFor({ state: "hidden" });
	assert.equal((await lastCall("bundle")).args.projects.length, 2);
	await open("context");
	await page.getByRole("button", { name: "Branch", exact: true }).click();
	await page
		.locator("#workspace-source-url")
		.fill("https://github.com/org/studio/tree/feature/review");
	await button("Validar").click();
	await page.getByText("Branch remota validada", { exact: true }).waitFor();
	await shot("source-light");
	await button("Abrir branch").click();
	await dialog.waitFor({ state: "hidden" });
	assert.equal((await lastCall("source")).args.projectId, "project-0");
	await open("clone");
	assert(await button("Clonar repositório").isDisabled());
	await page
		.locator("#workspace-repository-url")
		.fill("https://github.com/org/studio.git");
	await button("Escolher pasta").click();
	assert.equal(
		await page.locator("#workspace-project-id").inputValue(),
		"new-project",
	);
	await fits();
	await shot("clone-light");
	await button("Clonar repositório").click();
	await dialog.waitFor({ state: "hidden" });
	assert.equal((await lastCall("clone")).args.baseBranch, "");
	assert.equal(
		(await lastCall("clone")).args.workspaceRoot,
		"/fixture/new-project",
	);
	await open("clone");
	await button("Cancelar").click();
	await button("Tema").click();
	await button("Abrir modal").click();
	await page
		.locator("#workspace-repository-url")
		.fill("git@github.com:org/studio.git");
	await button("Escolher pasta").click();
	await shot("clone-dark");
	await page.setViewportSize({ width: 390, height: 740 });
	await page.emulateMedia({ reducedMotion: "reduce" });
	await fits();
	await shot("clone-compact");
	await page.locator("#workspace-name").scrollIntoViewIfNeeded();
	await fits();
	await shot("clone-footer-compact");
	await page.keyboard.press("Escape");
	await dialog.waitFor({ state: "hidden" });
	await open();
	await page.locator("#workspace-branch").waitFor();
	await fits();
	await shot("open-compact");
	await page.getByRole("button", { name: /Vários projetos/ }).click();
	await fits();
	await shot("multi-compact");
	await open("empty");
	await page.locator("#workspace-root").waitFor();
	await button("Escolher pasta").click();
	await page.waitForFunction(() =>
		document.querySelector("#workspace-branch")?.value?.startsWith("feature/"),
	);
	assert(await button("Criar tarefa").isEnabled());
	await open("picker-cancel");
	await button("Escolher pasta").click();
	assert.equal(await page.locator("#workspace-root").inputValue(), "");
	assert(await button("Criar tarefa").isDisabled());
	await open("branch-error");
	await page
		.getByText("Falha de leitura de demonstração.", { exact: true })
		.waitFor();
	assert(await button("Criar tarefa").isDisabled());
	await open("submit-error");
	await page
		.locator("#workspace-repository-url")
		.fill("https://github.com/org/studio.git");
	await button("Escolher pasta").click();
	await button("Clonar repositório").click();
	await page
		.getByText("Falha de criação de demonstração.", { exact: true })
		.waitFor();
	assert.equal(
		await page.locator("#workspace-repository-url").inputValue(),
		"https://github.com/org/studio.git",
	);
	assert(await button("Clonar repositório").isEnabled());
	assert.deepEqual(errors, []);
	console.log(
		"PASS: open/clone/bundle/source payloads, folder picker/cancel, pending/errors, keyboard dismissal, themes and compact layout. Screenshots: " +
			output,
	);
} catch (e) {
	console.error(await page.locator("body").innerText());
	await page.screenshot({ path: `${output}/failure.png` });
	throw e;
} finally {
	await browser.close();
}
